use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Sender;
use log::{error, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::audio::packet_ring::{
    accepted_frame_range, packet_ring, PacketConsumer, MAX_FRAMES_PER_PACKET,
};
use crate::audio::resampler::AudioResampler;

pub struct MicCapture {
    stream: cpal::Stream,
    stop_flag: Arc<AtomicBool>,
    stop_time: Arc<Mutex<Option<Instant>>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

// cpal::Stream contains pointer components that are !Send and !Sync on some platforms.
// Since we only control/drop it under standard Mutex locks within AppState, it is safe to impl Send/Sync.
unsafe impl Send for MicCapture {}
unsafe impl Sync for MicCapture {}

impl MicCapture {
    /// Stamp the exact end of the recording session. Frames captured after
    /// this instant are trimmed by the drain thread; call it the moment the
    /// user releases the hotkey, before the (slightly later) teardown.
    pub fn mark_stop(&self) {
        let mut stop = self.stop_time.lock().unwrap();
        if stop.is_none() {
            *stop = Some(Instant::now());
        }
    }
}

impl Drop for MicCapture {
    fn drop(&mut self) {
        info!("Stopping microphone capture...");
        self.mark_stop();
        let _ = self.stream.pause();
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        info!("Microphone capture stopped.");
    }
}

pub fn list_input_devices() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let devices = host.input_devices()?;
    let mut names = Vec::new();
    for device in devices {
        if let Ok(name) = device.name() {
            names.push(name);
        }
    }
    Ok(names)
}

fn resolve_device(device_name: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    match device_name {
        Some(name) => host
            .input_devices()?
            .find(|d| d.name().map(|n| n == name).unwrap_or(false))
            .ok_or_else(|| anyhow!("Device not found: {}", name)),
        None => host
            .default_input_device()
            .ok_or_else(|| anyhow!("No default input device found")),
    }
}

pub fn start_capture(
    device_name: Option<&str>,
    sender: Sender<Vec<f32>>,
    level_sender: Option<Sender<f32>>,
) -> Result<MicCapture> {
    let device = resolve_device(device_name)?;
    info!("Using input device: {}", device.name().unwrap_or_default());

    let config = device
        .default_input_config()
        .map_err(|e| anyhow!("Failed to get default input config: {}", e))?;

    let sample_format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();

    let input_sample_rate = stream_config.sample_rate.0;

    info!(
        "Input config: sample_rate={}, channels={}, format={:?}",
        input_sample_rate, stream_config.channels, sample_format
    );

    // Lock-free SPSC packet ring: the cpal callback owns the producer and
    // never allocates or locks; the drain thread below owns the consumer.
    let (producer, ring) = packet_ring(input_sample_rate);
    let stream = build_stream(&device, &stream_config, sample_format, producer)?;

    // Session start stamped just before hardware IO begins — the drain thread
    // trims any packet captured outside [start, stop].
    let session_start = Instant::now();
    stream.play()?;
    info!("CPAL audio stream started.");

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_time = Arc::new(Mutex::new(None::<Instant>));
    let stop_flag_clone = stop_flag.clone();
    let stop_time_clone = stop_time.clone();
    let thread_handle = thread::spawn(move || {
        run_drain_loop(
            ring,
            input_sample_rate,
            session_start,
            stop_time_clone,
            sender,
            level_sender,
            stop_flag_clone,
        );
    });

    Ok(MicCapture {
        stream,
        stop_flag,
        stop_time,
        thread_handle: Some(thread_handle),
    })
}

/// Drain the packet ring on a dedicated thread: trim each packet to the
/// session window (stale audio from a previous session or audio past the stop
/// mark can never leak through), resample to 16kHz mono, and forward chunks to
/// `sender` (+ optional RMS levels). Runs until `stop_flag` is set AND the
/// ring is empty, then flushes the resampler tail so the final phoneme isn't
/// clipped. Returns the consumer so a warm mic can reuse it next session.
fn run_drain_loop(
    mut ring: PacketConsumer,
    input_sample_rate: u32,
    session_start: Instant,
    stop_time: Arc<Mutex<Option<Instant>>>,
    sender: Sender<Vec<f32>>,
    level_sender: Option<Sender<f32>>,
    stop_flag: Arc<AtomicBool>,
) -> PacketConsumer {
    let chunk_size = 1024;
    let mut resampler = match AudioResampler::new(input_sample_rate, 16000, 1, chunk_size) {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to create resampler: {:?}", e);
            return ring;
        }
    };
    let frames_needed = resampler.input_frames_needed();
    let sample_rate = input_sample_rate as f64;

    let mut scratch = vec![0.0f32; MAX_FRAMES_PER_PACKET];
    let mut pending: Vec<f32> = Vec::with_capacity(frames_needed + MAX_FRAMES_PER_PACKET);

    // Resample one fixed-size input chunk and forward it. Returns false when
    // the receiving side hung up.
    let emit = |resampler: &mut AudioResampler, chunk: Vec<f32>| -> bool {
        match resampler.process(&[chunk]) {
            Ok(output_channels) => {
                if let Some(resampled_mono) = output_channels.first() {
                    if sender.send(resampled_mono.clone()).is_err() {
                        return false;
                    }
                    if let Some(ref l_sender) = level_sender {
                        // One RMS per 20ms sub-block (not per ~64ms chunk)
                        // so the UI waveform tracks the voice smoothly.
                        const LEVEL_BLOCK: usize = 320; // 20ms @ 16kHz
                        for block in resampled_mono.chunks(LEVEL_BLOCK) {
                            if block.is_empty() {
                                continue;
                            }
                            let sum_sq: f32 = block.iter().map(|&s| s * s).sum();
                            let _ = l_sender.send((sum_sq / block.len() as f32).sqrt());
                        }
                    }
                }
                true
            }
            Err(e) => {
                error!("Error during resampling: {:?}", e);
                true
            }
        }
    };

    'drain: loop {
        match ring.pop_into(&mut scratch) {
            Some((len, end_time)) => {
                let stop = *stop_time.lock().unwrap();
                if let Some(range) =
                    accepted_frame_range(len, sample_rate, end_time, session_start, stop)
                {
                    pending.extend_from_slice(&scratch[range]);
                }
                while pending.len() >= frames_needed {
                    let chunk: Vec<f32> = pending.drain(..frames_needed).collect();
                    if !emit(&mut resampler, chunk) {
                        break 'drain;
                    }
                }
            }
            None => {
                // Ring fully drained — exit only once stop is signalled, so
                // every packet captured before the stop mark gets delivered.
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                thread::sleep(Duration::from_millis(3));
            }
        }
        let dropped = ring.take_dropped();
        if dropped > 0 {
            warn!("Mic packet ring overflow: dropped {} packet(s).", dropped);
        }
    }

    // Flush the partial tail, padded with silence to a full resampler chunk.
    if !pending.is_empty() {
        pending.resize(frames_needed, 0.0);
        let _ = emit(&mut resampler, pending);
    }
    info!("Audio drain thread finished.");
    ring
}

/// A microphone stream that is **built once and kept paused** ("warm"), so that
/// starting a recording is near-instant — this is FluidVoice's trick: the slow
/// part (creating/initializing the CoreAudio input unit) happens ahead of time,
/// and pressing the hotkey only needs an `AudioDeviceStart` (~ms). While paused
/// the hardware IO is stopped, so macOS does NOT show the mic indicator — the
/// orange dot appears only between [`WarmMic::start`] and [`WarmMic::stop`].
pub struct WarmMic {
    stream: cpal::Stream,
    /// Consumer half of the packet ring; taken by each session's drain thread
    /// and returned when it exits. The producer lives inside the cpal callback.
    ring_slot: Arc<Mutex<Option<PacketConsumer>>>,
    input_sample_rate: u32,
    device_name: Option<String>,
    session: Mutex<Option<WarmSession>>,
}

struct WarmSession {
    stop_flag: Arc<AtomicBool>,
    stop_time: Arc<Mutex<Option<Instant>>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

// Same rationale as MicCapture: the cpal::Stream is only ever touched under our
// own locks in AppState, so it is safe to move across threads.
unsafe impl Send for WarmMic {}
unsafe impl Sync for WarmMic {}

impl WarmMic {
    /// Build (but do NOT start) the input stream for `device_name` (or the
    /// default device). Cheap to keep around; no mic indicator until `start`.
    pub fn build(device_name: Option<&str>) -> Result<WarmMic> {
        let device = resolve_device(device_name)?;
        let resolved_name = device.name().ok();
        let config = device
            .default_input_config()
            .map_err(|e| anyhow!("Failed to get default input config: {}", e))?;
        let sample_format = config.sample_format();
        let stream_config: cpal::StreamConfig = config.into();
        let input_sample_rate = stream_config.sample_rate.0;

        let (producer, ring) = packet_ring(input_sample_rate);
        let stream = build_stream(&device, &stream_config, sample_format, producer)?;
        // Deliberately NOT calling stream.play() — the stream stays paused (no
        // hardware IO, no mic indicator) until the first recording. Defensively
        // pause() too, so IO can never be left running at idle (no orange dot).
        let _ = stream.pause();
        info!(
            "WarmMic prepared: device={:?} (requested={:?}) sr={}",
            resolved_name, device_name, input_sample_rate
        );
        Ok(WarmMic {
            stream,
            ring_slot: Arc::new(Mutex::new(Some(ring))),
            input_sample_rate,
            // Store the REQUESTED name (not resolved) so "use default" (None)
            // compares equal across starts and doesn't force a rebuild each time.
            device_name: device_name.map(|s| s.to_string()),
            session: Mutex::new(None),
        })
    }

    /// The resolved device name this warm mic was built for (to detect changes).
    pub fn device_name(&self) -> Option<&str> {
        self.device_name.as_deref()
    }

    /// Start recording: `play()` the (already built) stream — near-instant — and
    /// spawn the drain thread that trims to this session's window and forwards
    /// 16kHz mono audio to `sender`. Any stale packets a previous session left
    /// in the ring predate the session start and are trimmed away, not delivered.
    pub fn start(&self, sender: Sender<Vec<f32>>, level_sender: Option<Sender<f32>>) -> Result<()> {
        let mut sess = self.session.lock().unwrap();
        if sess.is_some() {
            return Ok(()); // already recording
        }
        let ring = self
            .ring_slot
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| anyhow!("warm mic ring consumer unavailable"))?;

        let session_start = Instant::now();
        if let Err(e) = self.stream.play() {
            *self.ring_slot.lock().unwrap() = Some(ring);
            return Err(e.into());
        }
        info!("WarmMic: recording started (stream resumed).");

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_time = Arc::new(Mutex::new(None::<Instant>));
        let stop_clone = stop_flag.clone();
        let stop_time_clone = stop_time.clone();
        let ring_slot = self.ring_slot.clone();
        let isr = self.input_sample_rate;
        let handle = thread::spawn(move || {
            let ring = run_drain_loop(
                ring,
                isr,
                session_start,
                stop_time_clone,
                sender,
                level_sender,
                stop_clone,
            );
            // Hand the consumer back for the next warm session.
            *ring_slot.lock().unwrap() = Some(ring);
        });
        *sess = Some(WarmSession {
            stop_flag,
            stop_time,
            thread_handle: Some(handle),
        });
        Ok(())
    }

    /// Stamp the exact end of the current recording session (frames captured
    /// after this instant are trimmed). Safe to call before [`WarmMic::stop`];
    /// only the first mark per session sticks.
    pub fn mark_stop(&self) {
        if let Some(s) = self.session.lock().unwrap().as_ref() {
            let mut stop = s.stop_time.lock().unwrap();
            if stop.is_none() {
                *stop = Some(Instant::now());
            }
        }
    }

    /// Stop recording: pause hardware IO (mic indicator disappears) and stop the
    /// drain thread — but keep the stream BUILT so the next start is instant again.
    pub fn stop(&self) {
        let mut sess = self.session.lock().unwrap();
        if let Some(mut s) = sess.take() {
            {
                let mut stop = s.stop_time.lock().unwrap();
                if stop.is_none() {
                    *stop = Some(Instant::now());
                }
            }
            let _ = self.stream.pause(); // mic indicator disappears
            s.stop_flag.store(true, Ordering::Relaxed);
            if let Some(h) = s.thread_handle.take() {
                let _ = h.join(); // drains + trims the tail, returns the consumer
            }
            info!("WarmMic: recording stopped (stream paused, kept warm).");
        }
    }
}

impl Drop for WarmMic {
    fn drop(&mut self) {
        self.stop();
    }
}

trait ToF32: Copy {
    fn to_f32_val(self) -> f32;
}

impl ToF32 for f32 {
    fn to_f32_val(self) -> f32 {
        self
    }
}

impl ToF32 for i16 {
    fn to_f32_val(self) -> f32 {
        if self < 0 {
            self as f32 / 32768.0
        } else {
            self as f32 / 32767.0
        }
    }
}

impl ToF32 for u16 {
    fn to_f32_val(self) -> f32 {
        (self as f32 - 32768.0) / 32768.0
    }
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    producer: crate::audio::packet_ring::PacketProducer,
) -> Result<cpal::Stream> {
    match sample_format {
        cpal::SampleFormat::F32 => build_stream_typed::<f32>(device, config, producer),
        cpal::SampleFormat::I16 => build_stream_typed::<i16>(device, config, producer),
        cpal::SampleFormat::U16 => build_stream_typed::<u16>(device, config, producer),
        fmt => Err(anyhow!("Unsupported sample format: {:?}", fmt)),
    }
}

fn build_stream_typed<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut producer: crate::audio::packet_ring::PacketProducer,
) -> Result<cpal::Stream>
where
    T: cpal::Sample + cpal::SizedSample + ToF32,
{
    let channels = config.channels as usize;

    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            // Realtime path: downmix straight into a pre-allocated ring slot.
            // No allocation, no locks; overflow drops the packet and counts it.
            producer.push_downmix(data, channels, |s| s.to_f32_val(), Instant::now());
        },
        move |err| {
            error!("cpal input stream error: {:?}", err);
        },
        None,
    )?;

    Ok(stream)
}

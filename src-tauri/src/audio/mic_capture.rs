use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{error, info};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use anyhow::{anyhow, Result};
use crossbeam_channel::Sender;

use crate::audio::mixer::AudioMixer;
use crate::audio::resampler::AudioResampler;

pub struct MicCapture {
    stream: cpal::Stream,
    stop_flag: Arc<AtomicBool>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

// cpal::Stream contains pointer components that are !Send and !Sync on some platforms.
// Since we only control/drop it under standard Mutex locks within AppState, it is safe to impl Send/Sync.
unsafe impl Send for MicCapture {}
unsafe impl Sync for MicCapture {}


impl Drop for MicCapture {
    fn drop(&mut self) {
        info!("Stopping microphone capture...");
        self.stop_flag.store(true, Ordering::Relaxed);
        let _ = self.stream.pause();
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

pub fn start_capture(
    device_name: Option<&str>,
    sender: Sender<Vec<f32>>,
    level_sender: Option<Sender<f32>>,
) -> Result<MicCapture> {
    let host = cpal::default_host();
    
    let device = match device_name {
        Some(name) => host
            .input_devices()?
            .find(|d| d.name().map(|n| n == name).unwrap_or(false))
            .ok_or_else(|| anyhow!("Device not found: {}", name))?,
        None => host
            .default_input_device()
            .ok_or_else(|| anyhow!("No default input device found"))?,
    };

    info!("Using input device: {}", device.name().unwrap_or_default());

    let config = device
        .default_input_config()
        .map_err(|e| anyhow!("Failed to get default input config: {}", e))?;
    
    let sample_format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();
    
    let input_sample_rate = stream_config.sample_rate.0;
    let channels = stream_config.channels as usize;
    
    info!(
        "Input config: sample_rate={}, channels={}, format={:?}",
        input_sample_rate, channels, sample_format
    );

    // Create an audio mixer (ring buffer) with a 2-second buffer capacity
    let buffer_capacity = (input_sample_rate as usize) * channels * 2;
    let mixer = Arc::new(AudioMixer::new(buffer_capacity));
    
    // Build cpal input stream
    let mixer_clone = mixer.clone();
    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, mixer_clone)?,
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, mixer_clone)?,
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, mixer_clone)?,
        sample_fmt => return Err(anyhow!("Unsupported sample format: {:?}", sample_fmt)),
    };

    stream.play()?;
    info!("CPAL audio stream started.");

    // Setup Resampling & Processing Thread
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();
    let thread_handle = thread::spawn(move || {
        run_resampler_loop(mixer, input_sample_rate, sender, level_sender, stop_flag_clone);
    });

    Ok(MicCapture {
        stream,
        stop_flag,
        thread_handle: Some(thread_handle),
    })
}

/// Drain the raw-audio ring, resample to 16kHz mono, and forward chunks to
/// `sender` (+ optional RMS levels). Runs until `stop_flag` is set. Shared by
/// [`start_capture`] and [`WarmMic`].
fn run_resampler_loop(
    mixer: Arc<AudioMixer>,
    input_sample_rate: u32,
    sender: Sender<Vec<f32>>,
    level_sender: Option<Sender<f32>>,
    stop_flag: Arc<AtomicBool>,
) {
    let chunk_size = 1024;
    let mut resampler = match AudioResampler::new(input_sample_rate, 16000, 1, chunk_size) {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to create resampler: {:?}", e);
            return;
        }
    };
    let frames_needed = resampler.input_frames_needed();
    let mut raw_buffer = vec![0.0f32; frames_needed];

    while !stop_flag.load(Ordering::Relaxed) {
        if mixer.available_samples() >= frames_needed {
            let read = mixer.pop_samples(&mut raw_buffer);
            if read == frames_needed {
                let input_channels = vec![raw_buffer.clone()];
                match resampler.process(&input_channels) {
                    Ok(output_channels) => {
                        if let Some(resampled_mono) = output_channels.first() {
                            if sender.send(resampled_mono.clone()).is_err() {
                                break;
                            }
                            if let Some(ref l_sender) = level_sender {
                                // One RMS per 20ms sub-block (not per ~64ms chunk)
                                // so the UI waveform tracks the voice smoothly.
                                const LEVEL_BLOCK: usize = 320; // 20ms @ 16kHz
                                for block in resampled_mono.chunks(LEVEL_BLOCK) {
                                    if block.is_empty() { continue; }
                                    let sum_sq: f32 = block.iter().map(|&s| s * s).sum();
                                    let _ = l_sender.send((sum_sq / block.len() as f32).sqrt());
                                }
                            }
                        }
                    }
                    Err(e) => error!("Error during resampling: {:?}", e),
                }
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }
    info!("Audio processing thread finished.");
}

/// A microphone stream that is **built once and kept paused** ("warm"), so that
/// starting a recording is near-instant — this is FluidVoice's trick: the slow
/// part (creating/initializing the CoreAudio input unit) happens ahead of time,
/// and pressing the hotkey only needs an `AudioDeviceStart` (~ms). While paused
/// the hardware IO is stopped, so macOS does NOT show the mic indicator — the
/// orange dot appears only between [`WarmMic::start`] and [`WarmMic::stop`].
pub struct WarmMic {
    stream: cpal::Stream,
    mixer: Arc<AudioMixer>,
    input_sample_rate: u32,
    device_name: Option<String>,
    session: std::sync::Mutex<Option<WarmSession>>,
}

struct WarmSession {
    stop_flag: Arc<AtomicBool>,
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
        let host = cpal::default_host();
        let device = match device_name {
            Some(name) => host
                .input_devices()?
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .ok_or_else(|| anyhow!("Device not found: {}", name))?,
            None => host
                .default_input_device()
                .ok_or_else(|| anyhow!("No default input device found"))?,
        };
        let resolved_name = device.name().ok();
        let config = device
            .default_input_config()
            .map_err(|e| anyhow!("Failed to get default input config: {}", e))?;
        let sample_format = config.sample_format();
        let stream_config: cpal::StreamConfig = config.into();
        let input_sample_rate = stream_config.sample_rate.0;
        let channels = stream_config.channels as usize;

        let buffer_capacity = (input_sample_rate as usize) * channels * 2;
        let mixer = Arc::new(AudioMixer::new(buffer_capacity));
        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, mixer.clone())?,
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, mixer.clone())?,
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, mixer.clone())?,
            sample_fmt => return Err(anyhow!("Unsupported sample format: {:?}", sample_fmt)),
        };
        // Deliberately NOT calling stream.play() — the stream stays paused (no
        // hardware IO, no mic indicator) until the first recording. Defensively
        // pause() too, so IO can never be left running at idle (no orange dot).
        let _ = stream.pause();
        info!("WarmMic prepared: device={:?} (requested={:?}) sr={}", resolved_name, device_name, input_sample_rate);
        Ok(WarmMic {
            stream,
            mixer,
            input_sample_rate,
            // Store the REQUESTED name (not resolved) so "use default" (None)
            // compares equal across starts and doesn't force a rebuild each time.
            device_name: device_name.map(|s| s.to_string()),
            session: std::sync::Mutex::new(None),
        })
    }

    /// The resolved device name this warm mic was built for (to detect changes).
    pub fn device_name(&self) -> Option<&str> {
        self.device_name.as_deref()
    }

    /// Start recording: `play()` the (already built) stream — near-instant — and
    /// spawn the resampler that forwards 16kHz mono audio to `sender`.
    pub fn start(&self, sender: Sender<Vec<f32>>, level_sender: Option<Sender<f32>>) -> Result<()> {
        let mut sess = self.session.lock().unwrap();
        if sess.is_some() {
            return Ok(()); // already recording
        }
        self.mixer.clear(); // drop any stale samples before this session
        self.stream.play()?; // mic indicator appears now
        info!("WarmMic: recording started (stream resumed).");
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_flag.clone();
        let mixer = self.mixer.clone();
        let isr = self.input_sample_rate;
        let handle = thread::spawn(move || {
            run_resampler_loop(mixer, isr, sender, level_sender, stop_clone);
        });
        *sess = Some(WarmSession { stop_flag, thread_handle: Some(handle) });
        Ok(())
    }

    /// Stop recording: pause hardware IO (mic indicator disappears) and stop the
    /// resampler — but keep the stream BUILT so the next start is instant again.
    pub fn stop(&self) {
        let mut sess = self.session.lock().unwrap();
        if let Some(mut s) = sess.take() {
            s.stop_flag.store(true, Ordering::Relaxed);
            let _ = self.stream.pause(); // mic indicator disappears
            if let Some(h) = s.thread_handle.take() {
                let _ = h.join();
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

trait ToF32 {
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

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mixer: Arc<AudioMixer>,
) -> Result<cpal::Stream>
where
    T: cpal::Sample + cpal::SizedSample + ToF32,
{
    let channels = config.channels as usize;
    let mixer_clone = mixer.clone();

    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            // Convert multi-channel input to mono f32
            let mut mono_samples = Vec::with_capacity(data.len() / channels);
            for chunk in data.chunks_exact(channels) {
                let sum: f32 = chunk.iter().map(|&s| s.to_f32_val()).sum();
                mono_samples.push(sum / (channels as f32));
            }
            let _ = mixer_clone.push_samples(&mono_samples);
        },
        move |err| {
            error!("cpal input stream error: {:?}", err);
        },
        None,
    )?;

    Ok(stream)
}

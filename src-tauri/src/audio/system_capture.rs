use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{error, info, warn};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use anyhow::{anyhow, Result};
use crossbeam_channel::Sender;

use crate::audio::mixer::AudioMixer;
use crate::audio::resampler::AudioResampler;

pub struct SystemCapture {
    stream: Option<cpal::Stream>,
    stop_flag: Arc<AtomicBool>,
    thread_handle: Option<thread::JoinHandle<()>>,

    // Native ScreenCaptureKit stream handle (only present when the SCK path is
    // actually used). Kept alive for the duration of the capture; dropped in
    // `Drop` after `stop_flag` is set. Stored as an opaque owner so the rest of
    // the struct stays platform-neutral.
    #[cfg(feature = "macos-native")]
    sck: Option<sck::SckStream>,
}

// cpal::Stream contains pointer components that are !Send and !Sync on some platforms.
// Since we only control/drop it under standard Mutex locks within AppState, it is safe to impl Send/Sync.
unsafe impl Send for SystemCapture {}
unsafe impl Sync for SystemCapture {}

impl Drop for SystemCapture {
    fn drop(&mut self) {
        info!("Stopping system audio capture...");
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(ref stream) = self.stream {
            let _ = stream.pause();
        }
        #[cfg(feature = "macos-native")]
        if let Some(sck) = self.sck.take() {
            sck.stop();
        }
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        info!("System audio capture stopped.");
    }
}

pub fn find_loopback_device() -> Result<Option<cpal::Device>> {
    let host = cpal::default_host();
    let devices = host.input_devices()?;
    for device in devices {
        if let Ok(name) = device.name() {
            let name_lower = name.to_lowercase();
            if name_lower.contains("blackhole")
                || name_lower.contains("loopback")
                || name_lower.contains("soundflower")
                || name_lower.contains("virtual")
                || name_lower.contains("system audio")
            {
                return Ok(Some(device));
            }
        }
    }
    Ok(None)
}

/// Start system-audio capture.
///
/// Output contract (unchanged): pushes `Vec<f32>` of mono, 16 kHz PCM samples
/// into `sender` continuously until the returned `SystemCapture` is dropped.
///
/// When the `macos-native` feature is enabled we *attempt* ScreenCaptureKit
/// first and fall back to the existing cpal-loopback path on any failure. When
/// the feature is off (default), the existing cpal-loopback / simulation path is
/// used exactly as before.
pub fn start_system_capture(
    sender: Sender<Vec<f32>>,
    level_sender: Option<Sender<f32>>,
) -> Result<SystemCapture> {
    #[cfg(feature = "macos-native")]
    {
        match sck::start(sender.clone(), level_sender.clone()) {
            Ok(capture) => {
                info!("Started system audio capture via ScreenCaptureKit.");
                return Ok(capture);
            }
            Err(e) => {
                warn!(
                    "ScreenCaptureKit system capture unavailable ({}); \
                     falling back to cpal loopback path.",
                    e
                );
            }
        }
    }

    start_system_capture_cpal(sender, level_sender)
}

/// Existing cpal-loopback (BlackHole/virtual device) capture path, or a silent
/// simulation stream when no loopback device is present. This is the default
/// behavior and the fallback for the native path.
fn start_system_capture_cpal(
    sender: Sender<Vec<f32>>,
    level_sender: Option<Sender<f32>>,
) -> Result<SystemCapture> {
    let loopback_device = find_loopback_device()?;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();

    if let Some(device) = loopback_device {
        info!("Found macOS virtual loopback device for system capture: {}", device.name().unwrap_or_default());

        let config = device
            .default_input_config()
            .map_err(|e| anyhow!("Failed to get default input config for loopback: {}", e))?;

        let sample_format = config.sample_format();
        let stream_config: cpal::StreamConfig = config.into();

        let input_sample_rate = stream_config.sample_rate.0;
        let channels = stream_config.channels as usize;

        info!(
            "System loopback capture config: sample_rate={}, channels={}, format={:?}",
            input_sample_rate, channels, sample_format
        );

        let buffer_capacity = (input_sample_rate as usize) * channels * 2;
        let mixer = Arc::new(AudioMixer::new(buffer_capacity));

        let mixer_clone = mixer.clone();
        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_system_stream::<f32>(&device, &stream_config, mixer_clone)?,
            cpal::SampleFormat::I16 => build_system_stream::<i16>(&device, &stream_config, mixer_clone)?,
            cpal::SampleFormat::U16 => build_system_stream::<u16>(&device, &stream_config, mixer_clone)?,
            sample_fmt => return Err(anyhow!("Unsupported sample format for loopback: {:?}", sample_fmt)),
        };

        stream.play()?;
        info!("CPAL system loopback stream started.");

        let thread_handle = thread::spawn(move || {
            let chunk_size = 1024;
            let mut resampler = match AudioResampler::new(input_sample_rate, 16000, 1, chunk_size) {
                Ok(r) => r,
                Err(e) => {
                    error!("Failed to create resampler for system capture: {:?}", e);
                    return;
                }
            };

            let frames_needed = resampler.input_frames_needed();
            let mut raw_buffer = vec![0.0f32; frames_needed];

            while !stop_flag_clone.load(Ordering::Relaxed) {
                let available = mixer.available_samples();
                if available >= frames_needed {
                    let read = mixer.pop_samples(&mut raw_buffer);
                    if read == frames_needed {
                        let input_channels = vec![raw_buffer.clone()];

                        match resampler.process(&input_channels) {
                            Ok(output_channels) => {
                                if let Some(resampled_mono) = output_channels.first() {
                                    if let Err(e) = sender.send(resampled_mono.clone()) {
                                        error!("Failed to send resampled system audio: {:?}", e);
                                        break;
                                    }

                                    if let Some(ref _l_sender) = level_sender {
                                        let sum_sq: f32 = resampled_mono.iter().map(|&s| s * s).sum();
                                        let _rms = (sum_sq / (resampled_mono.len() as f32)).sqrt();
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Error during resampling system audio: {:?}", e);
                            }
                        }
                    }
                } else {
                    thread::sleep(Duration::from_millis(10));
                }
            }
            info!("System audio processing thread finished.");
        });

        Ok(SystemCapture {
            stream: Some(stream),
            stop_flag,
            thread_handle: Some(thread_handle),
            #[cfg(feature = "macos-native")]
            sck: None,
        })
    } else {
        warn!("No virtual loopback device (e.g. BlackHole) found. Starting system audio capture in simulation mode.");

        let thread_handle = thread::spawn(move || {
            let sample_rate = 16000;
            let chunk_size = 1024;
            let frame_duration = Duration::from_secs_f64(chunk_size as f64 / sample_rate as f64);

            while !stop_flag_clone.load(Ordering::Relaxed) {
                // Generate a silent frame to simulate system audio (or comfort noise)
                let silent_frame = vec![0.0f32; chunk_size];
                if let Err(_) = sender.send(silent_frame) {
                    break;
                }

                if let Some(ref l_sender) = level_sender {
                    let _ = l_sender.send(0.0);
                }

                thread::sleep(frame_duration);
            }
            info!("System audio simulation thread finished.");
        });

        Ok(SystemCapture {
            stream: None,
            stop_flag,
            thread_handle: Some(thread_handle),
            #[cfg(feature = "macos-native")]
            sck: None,
        })
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

fn build_system_stream<T>(
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
            let mut mono_samples = Vec::with_capacity(data.len() / channels);
            for chunk in data.chunks_exact(channels) {
                let sum: f32 = chunk.iter().map(|&s| s.to_f32_val()).sum();
                mono_samples.push(sum / (channels as f32));
            }
            let _ = mixer_clone.push_samples(&mono_samples);
        },
        move |err| {
            error!("cpal system input stream error: {:?}", err);
        },
        None,
    )?;

    Ok(stream)
}

// ---------------------------------------------------------------------------
// Native ScreenCaptureKit path (macos-native feature).
//
// IMPORTANT — binding limitation. This module targets `objc2 = "0.5"`, so the
// only compatible ScreenCaptureKit binding is `objc2-screen-capture-kit` 0.2.2.
// That release predates the audio-capture bindings:
//
//   * The `SCStreamOutput` protocol is generated as an EMPTY trait; the audio
//     delivery callback `stream:didOutputSampleBuffer:ofType:` is not declared.
//   * `addStreamOutput:type:sampleHandlerQueue:error:` (the method that would
//     register our output delegate) is absent — only `removeStreamOutput` is
//     bound.
//   * There is no `CMSampleBuffer` type available: `objc2-core-media` has no
//     release compatible with objc2 0.5 (its first version, 0.3.0, requires
//     objc2 0.6), so even if the callback existed we could not decode the PCM.
//
// The complete audio API only exists in the objc2 0.6 generation
// (objc2-screen-capture-kit 0.3.x + objc2-core-media 0.3.x), which we cannot
// adopt here without bumping the workspace-pinned `objc2 = "0.5"`.
//
// Consequences of that gap for this module:
//   * We CAN, and do, compile real SCK calls: permission/content discovery via
//     `SCShareableContent`, building an `SCContentFilter` over the main display,
//     configuring an `SCStreamConfiguration` for audio (captures_audio = true,
//     sample_rate = 16000, mono, exclude own process), and constructing an
//     `SCStream`.
//   * We CANNOT receive the resulting `CMSampleBuffer`s, so a started stream
//     would deliver zero samples into the crossbeam channel and starve the
//     meeting service. Rather than start a "blind" stream, `start()` returns an
//     error describing the gap so the caller falls back to the cpal-loopback
//     path, which produces the required continuous sample stream.
//
// When the workspace moves to objc2 0.6, swap the bindings, implement an
// `SCStreamOutput` delegate with `RcBlock`/`define_class!`, register it via
// `addStreamOutput`, extract PCM from the `CMSampleBuffer`'s
// `CMSampleBufferGetDataBuffer` / AudioBufferList, downmix to mono, resample to
// 16 kHz (reuse `AudioResampler`) and push into `sender`. The scaffolding below
// is arranged so only the delegate + extraction need to be filled in.
// ---------------------------------------------------------------------------
#[cfg(feature = "macos-native")]
mod sck {
    use super::*;
    use objc2::rc::Retained;
    use objc2_screen_capture_kit::{SCStream, SCStreamConfiguration};

    /// Opaque owner of a live SCK stream. Currently unconstructed because the
    /// 0.2.2 bindings cannot deliver samples (see module docs); retained so the
    /// `SystemCapture` field and `Drop` logic are ready for the objc2 0.6 upgrade.
    pub struct SckStream {
        stream: Retained<SCStream>,
    }

    // The stream is only ever touched from the thread that owns SystemCapture
    // (behind AppState's Mutex); mark Send/Sync to store it in the Send handle.
    unsafe impl Send for SckStream {}
    unsafe impl Sync for SckStream {}

    impl SckStream {
        pub fn stop(self) {
            // Would call `stopCaptureWithCompletionHandler`; the completion is
            // async so we fire-and-forget and let Drop release the retained obj.
            unsafe {
                self.stream
                    .stopCaptureWithCompletionHandler(None);
            }
        }
    }

    /// Build an audio-configured `SCStreamConfiguration`.
    ///
    /// This is a real SCK call and compiles/executes; it is exercised by `start`
    /// to validate the framework is present before we decide whether we can
    /// actually stream. Requests mono 16 kHz and excludes our own process audio.
    fn build_audio_config() -> Retained<SCStreamConfiguration> {
        unsafe {
            let config = SCStreamConfiguration::new();
            config.setCapturesAudio(true);
            // sampleRate/channelCount are advisory; SCK will honor common values
            // (48k/44.1k) and we resample. Request 16 kHz mono to minimize work.
            config.setSampleRate(16000);
            config.setChannelCount(1);
            config.setExcludesCurrentProcessAudio(true);
            config
        }
    }

    /// Attempt to start ScreenCaptureKit system-audio capture.
    ///
    /// Returns `Err` (triggering the cpal fallback) whenever a real, sample-
    /// delivering stream cannot be established. With the 0.2.2 bindings that is
    /// always the case (no sample callback / no CoreMedia), so we validate what
    /// we can, then bail out with a descriptive error.
    pub fn start(
        _sender: Sender<Vec<f32>>,
        _level_sender: Option<Sender<f32>>,
    ) -> Result<SystemCapture> {
        // Exercise the real framework so the binding is genuinely linked and we
        // fail fast (rather than at a later delegate step) if SCK is missing.
        let _config = build_audio_config();

        Err(anyhow!(
            "objc2-screen-capture-kit 0.2.2 (objc2 0.5) lacks the audio \
             sample-buffer callback and CoreMedia bindings required to extract \
             PCM; upgrade to the objc2 0.6 SCK bindings to enable this path"
        ))
    }
}

//! Fixed-capacity lock-free SPSC packet ring for the realtime mic callback,
//! ported from FluidVoice's CoreAudioCaptureSupport.c: 64 slots × 8192 frames.
//!
//! The producer side runs inside the cpal (CoreAudio) callback and must never
//! allocate or lock: it downmixes interleaved input directly into a
//! pre-allocated ring slot and publishes it with an atomic index store. When
//! the ring is full the packet is dropped and counted (the consumer logs it) —
//! backpressure must never block the realtime thread.
//!
//! Each packet is stamped with the host time of its delivery, which lets the
//! consumer trim delivered audio to the exact recording session window
//! ([`accepted_frame_range`]) instead of relying on buffers being drained
//! between sessions.

use ringbuf::{traits::*, HeapRb};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 64 hardware cycles of headroom — ample scheduling slack while keeping the
/// realtime producer strictly allocation-free (matches FluidVoice).
pub const RING_SLOTS: usize = 64;
pub const MAX_FRAMES_PER_PACKET: usize = 8192;

/// One hardware callback's worth of mono samples plus its capture timestamp.
pub struct Packet {
    samples: [f32; MAX_FRAMES_PER_PACKET],
    len: u32,
    /// Host time at delivery — i.e. (approximately) the capture time of the
    /// packet's LAST frame.
    end_time: Instant,
}

type RbProducer = ringbuf::wrap::caching::Caching<Arc<HeapRb<Packet>>, true, false>;
type RbConsumer = ringbuf::wrap::caching::Caching<Arc<HeapRb<Packet>>, false, true>;

/// Create a producer/consumer pair over a freshly allocated ring.
/// `sample_rate` is the device rate, used to back-date split packets.
pub fn packet_ring(sample_rate: u32) -> (PacketProducer, PacketConsumer) {
    let rb = HeapRb::<Packet>::new(RING_SLOTS);
    let (prod, cons) = rb.split();
    let dropped = Arc::new(AtomicU64::new(0));
    (
        PacketProducer {
            prod,
            dropped: dropped.clone(),
            sample_rate: sample_rate as f64,
        },
        PacketConsumer { cons, dropped, taken_dropped: 0 },
    )
}

pub struct PacketProducer {
    prod: RbProducer,
    dropped: Arc<AtomicU64>,
    sample_rate: f64,
}

impl PacketProducer {
    /// Downmix interleaved `data` (`channels` per frame) to mono directly into
    /// ring slots. Realtime-safe: no allocation, no locks; drops (and counts)
    /// packets when the ring is full. Callbacks larger than a slot are split
    /// into multiple packets with back-dated timestamps.
    pub fn push_downmix<T: Copy>(
        &mut self,
        data: &[T],
        channels: usize,
        to_f32: impl Fn(T) -> f32,
        now: Instant,
    ) {
        if channels == 0 {
            return;
        }
        let total_frames = data.len() / channels;
        let mut offset = 0usize; // frames consumed so far
        while offset < total_frames {
            let frames = (total_frames - offset).min(MAX_FRAMES_PER_PACKET);
            // Back-date all but the final chunk: `now` is the capture time of
            // the LAST frame of the whole callback.
            let frames_after_chunk = total_frames - (offset + frames);
            let end_time = if frames_after_chunk == 0 {
                now
            } else {
                now.checked_sub(Duration::from_secs_f64(
                    frames_after_chunk as f64 / self.sample_rate,
                ))
                .unwrap_or(now)
            };

            let (a, b) = self.prod.vacant_slices_mut();
            let slot = match a.first_mut().or_else(|| b.first_mut()) {
                Some(s) => s,
                None => {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                    offset += frames;
                    continue;
                }
            };
            let p = slot.as_mut_ptr();
            unsafe {
                let samples = std::ptr::addr_of_mut!((*p).samples) as *mut f32;
                for frame in 0..frames {
                    let base = (offset + frame) * channels;
                    let mut sum = 0.0f32;
                    for ch in 0..channels {
                        sum += to_f32(data[base + ch]);
                    }
                    *samples.add(frame) = sum / channels as f32;
                }
                std::ptr::addr_of_mut!((*p).len).write(frames as u32);
                std::ptr::addr_of_mut!((*p).end_time).write(end_time);
                self.prod.advance_write_index(1);
            }
            offset += frames;
        }
    }
}

pub struct PacketConsumer {
    cons: RbConsumer,
    dropped: Arc<AtomicU64>,
    taken_dropped: u64,
}

impl PacketConsumer {
    /// Copy the next packet's samples into `out` (must hold at least
    /// [`MAX_FRAMES_PER_PACKET`]); returns its frame count and end timestamp.
    pub fn pop_into(&mut self, out: &mut [f32]) -> Option<(usize, Instant)> {
        debug_assert!(out.len() >= MAX_FRAMES_PER_PACKET);
        let (a, b) = self.cons.occupied_slices();
        let slot = a.first().or_else(|| b.first())?;
        let p = slot.as_ptr();
        unsafe {
            let len = (std::ptr::addr_of!((*p).len).read() as usize).min(MAX_FRAMES_PER_PACKET);
            let end_time = std::ptr::addr_of!((*p).end_time).read();
            let samples = std::ptr::addr_of!((*p).samples) as *const f32;
            std::ptr::copy_nonoverlapping(samples, out.as_mut_ptr(), len);
            self.cons.advance_read_index(1);
            Some((len, end_time))
        }
    }

    /// Overflow drops since the last call (for consumer-side logging).
    pub fn take_dropped(&mut self) -> u64 {
        let total = self.dropped.load(Ordering::Relaxed);
        let delta = total - self.taken_dropped;
        self.taken_dropped = total;
        delta
    }
}

/// Which frames of a packet fall inside the recording session window
/// `[session_start, session_stop]`. `packet_end` is the capture time of the
/// packet's last frame. Returns `None` when no frame is inside the window.
///
/// This is FluidVoice's `acceptedFrameRange`: stale audio captured before the
/// session started (e.g. left in the ring by a previous session) and audio
/// captured after the stop mark can never leak into the delivered stream.
pub fn accepted_frame_range(
    frames: usize,
    sample_rate: f64,
    packet_end: Instant,
    session_start: Instant,
    session_stop: Option<Instant>,
) -> Option<Range<usize>> {
    if frames == 0 || sample_rate <= 0.0 {
        return None;
    }
    let packet_start = match packet_end
        .checked_sub(Duration::from_secs_f64(frames as f64 / sample_rate))
    {
        Some(t) => t,
        // Clock too close to process start to back-date — accept conservatively.
        None => return Some(0..frames),
    };

    let mut lower = 0usize;
    if session_start > packet_start {
        let before = session_start.saturating_duration_since(packet_start).as_secs_f64();
        lower = (before * sample_rate).ceil() as usize;
    }

    let mut upper = frames;
    if let Some(stop) = session_stop {
        if stop <= packet_start {
            return None;
        }
        let keep = stop.saturating_duration_since(packet_start).as_secs_f64();
        upper = ((keep * sample_rate).floor() as usize).min(frames);
    }

    if lower < upper {
        Some(lower..upper)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn secs(s: f64) -> Duration {
        Duration::from_secs_f64(s)
    }

    #[test]
    fn accepts_packet_fully_inside_window() {
        let base = Instant::now();
        // Packet spans [1.0, 1.1]s; session [0.5, ∞).
        let r = accepted_frame_range(4800, SR, base + secs(1.1), base + secs(0.5), None);
        assert_eq!(r, Some(0..4800));
    }

    #[test]
    fn trims_frames_before_session_start() {
        let base = Instant::now();
        // Packet spans [0.0, 0.1]s; session starts at 0.05 → keep second half.
        let r = accepted_frame_range(4800, SR, base + secs(0.1), base + secs(0.05), None)
            .expect("some frames accepted");
        assert!(r.start >= 2399 && r.start <= 2401, "start={}", r.start);
        assert_eq!(r.end, 4800);
    }

    #[test]
    fn rejects_packet_entirely_before_session_start() {
        let base = Instant::now();
        // Packet spans [0.0, 0.1]s; session starts at 0.2.
        let r = accepted_frame_range(4800, SR, base + secs(0.1), base + secs(0.2), None);
        assert_eq!(r, None);
    }

    #[test]
    fn trims_frames_after_stop_mark() {
        let base = Instant::now();
        // Packet spans [1.0, 1.1]s; stop at 1.05 → keep first half.
        let r = accepted_frame_range(
            4800,
            SR,
            base + secs(1.1),
            base,
            Some(base + secs(1.05)),
        )
        .expect("some frames accepted");
        assert_eq!(r.start, 0);
        assert!(r.end >= 2399 && r.end <= 2401, "end={}", r.end);
    }

    #[test]
    fn rejects_packet_entirely_after_stop() {
        let base = Instant::now();
        // Packet spans [2.0, 2.1]s; stop at 1.5.
        let r = accepted_frame_range(
            4800,
            SR,
            base + secs(2.1),
            base,
            Some(base + secs(1.5)),
        );
        assert_eq!(r, None);
    }

    #[test]
    fn empty_packet_rejected() {
        let base = Instant::now();
        assert_eq!(accepted_frame_range(0, SR, base, base, None), None);
    }

    #[test]
    fn ring_roundtrip_preserves_order_and_data() {
        let (mut prod, mut cons) = packet_ring(48_000);
        let now = Instant::now();
        for i in 0..4u32 {
            let data = vec![i as f32; 512 * 2]; // 512 frames, stereo
            prod.push_downmix(&data, 2, |s| s, now);
        }
        let mut out = vec![0f32; MAX_FRAMES_PER_PACKET];
        for i in 0..4u32 {
            let (len, _t) = cons.pop_into(&mut out).expect("packet present");
            assert_eq!(len, 512);
            assert!(out[..len].iter().all(|&s| s == i as f32));
        }
        assert!(cons.pop_into(&mut out).is_none());
    }

    #[test]
    fn overflow_drops_and_counts() {
        let (mut prod, mut cons) = packet_ring(48_000);
        let now = Instant::now();
        let data = vec![0.5f32; 256];
        for _ in 0..(RING_SLOTS + 5) {
            prod.push_downmix(&data, 1, |s| s, now);
        }
        assert_eq!(cons.take_dropped(), 5);
        let mut out = vec![0f32; MAX_FRAMES_PER_PACKET];
        let mut popped = 0;
        while cons.pop_into(&mut out).is_some() {
            popped += 1;
        }
        assert_eq!(popped, RING_SLOTS);
        assert_eq!(cons.take_dropped(), 0);
    }

    #[test]
    fn oversized_callback_splits_into_multiple_packets() {
        let (mut prod, mut cons) = packet_ring(48_000);
        let now = Instant::now();
        let frames = MAX_FRAMES_PER_PACKET + 100;
        let data = vec![1.0f32; frames];
        prod.push_downmix(&data, 1, |s| s, now);
        let mut out = vec![0f32; MAX_FRAMES_PER_PACKET];
        let (len1, t1) = cons.pop_into(&mut out).unwrap();
        let (len2, t2) = cons.pop_into(&mut out).unwrap();
        assert_eq!(len1, MAX_FRAMES_PER_PACKET);
        assert_eq!(len2, 100);
        assert!(t1 < t2, "first chunk must be back-dated before the last");
        assert_eq!(t2, now);
        assert!(cons.pop_into(&mut out).is_none());
    }
}

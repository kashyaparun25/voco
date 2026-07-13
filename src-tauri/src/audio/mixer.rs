use ringbuf::{HeapRb, traits::*};
use parking_lot::Mutex;
use std::sync::Arc;

pub struct AudioMixer {
    // HeapRb split producer for writing raw audio from CPAL thread
    mic_producer: Arc<Mutex<ringbuf::wrap::caching::Caching<Arc<HeapRb<f32>>, true, false>>>,
    // HeapRb split consumer for reading raw audio in the processing thread
    mic_consumer: Arc<Mutex<ringbuf::wrap::caching::Caching<Arc<HeapRb<f32>>, false, true>>>,
}

impl AudioMixer {
    pub fn new(capacity: usize) -> Self {
        let rb = HeapRb::<f32>::new(capacity);
        let (prod, cons) = rb.split();
        Self {
            mic_producer: Arc::new(Mutex::new(prod)),
            mic_consumer: Arc::new(Mutex::new(cons)),
        }
    }

    pub fn push_samples(&self, samples: &[f32]) -> usize {
        let mut prod = self.mic_producer.lock();
        prod.push_slice(samples)
    }

    pub fn pop_samples(&self, buffer: &mut [f32]) -> usize {
        let mut cons = self.mic_consumer.lock();
        cons.pop_slice(buffer)
    }

    pub fn available_samples(&self) -> usize {
        let cons = self.mic_consumer.lock();
        cons.occupied_len()
    }

    /// Drop any buffered samples (used when (re)starting a warm-mic session so
    /// stale audio from a previous session isn't prepended).
    pub fn clear(&self) {
        let mut cons = self.mic_consumer.lock();
        cons.clear();
    }
}

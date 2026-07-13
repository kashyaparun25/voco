use rubato::{FftFixedInOut, Resampler};
use anyhow::Result;

pub struct AudioResampler {
    resampler: Option<FftFixedInOut<f32>>,
    _input_sample_rate: u32,
    _output_sample_rate: u32,
    _channels: usize,
    chunk_size: usize,
}

impl AudioResampler {
    pub fn new(input_sample_rate: u32, output_sample_rate: u32, channels: usize, chunk_size: usize) -> Result<Self> {
        if input_sample_rate == output_sample_rate && channels == 1 {
            return Ok(Self {
                resampler: None,
                _input_sample_rate: input_sample_rate,
                _output_sample_rate: output_sample_rate,
                _channels: channels,
                chunk_size,
            });
        }

        let resampler = FftFixedInOut::<f32>::new(
            input_sample_rate as usize,
            output_sample_rate as usize,
            chunk_size,
            channels,
        )?;

        Ok(Self {
            resampler: Some(resampler),
            _input_sample_rate: input_sample_rate,
            _output_sample_rate: output_sample_rate,
            _channels: channels,
            chunk_size,
        })
    }

    pub fn input_frames_needed(&self) -> usize {
        if let Some(ref r) = self.resampler {
            r.input_frames_next()
        } else {
            self.chunk_size
        }
    }

    pub fn process(&mut self, input: &[Vec<f32>]) -> Result<Vec<Vec<f32>>> {
        if let Some(ref mut r) = self.resampler {
            let output = r.process(input, None)?;
            Ok(output)
        } else {
            Ok(input.to_vec())
        }
    }
}

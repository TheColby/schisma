//! Non-realtime spectrum, level, loudness, and stereo-correlation analysis.

use realfft::{num_complex::Complex32, RealFftPlanner, RealToComplex};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AnalysisSnapshot {
    pub spectrum_db: Vec<f32>,
    pub peak_dbfs: [f32; 2],
    pub rms_dbfs: [f32; 2],
    pub momentary_lufs: f32,
    pub stereo_correlation: f32,
}

impl AnalysisSnapshot {
    pub fn silence(spectrum_bins: usize) -> Self {
        Self {
            spectrum_db: vec![-120.0; spectrum_bins],
            peak_dbfs: [-120.0; 2],
            rms_dbfs: [-120.0; 2],
            momentary_lufs: -120.0,
            stereo_correlation: 0.0,
        }
    }
}

pub struct Analyzer {
    sample_rate: f32,
    fft_size: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    fft_input: Vec<f32>,
    fft_output: Vec<Complex32>,
    window: Vec<f32>,
}

impl Analyzer {
    pub fn new(sample_rate: f32, fft_size: usize) -> Self {
        assert!(sample_rate.is_finite() && sample_rate > 0.0);
        assert!(fft_size >= 64 && fft_size.is_power_of_two());
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);
        let fft_input = fft.make_input_vec();
        let fft_output = fft.make_output_vec();
        let window = (0..fft_size)
            .map(|index| {
                0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / (fft_size - 1) as f32).cos()
            })
            .collect();
        Self {
            sample_rate,
            fft_size,
            fft,
            fft_input,
            fft_output,
            window,
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn fft_size(&self) -> usize {
        self.fft_size
    }

    pub fn analyze(&mut self, frames: &[[f32; 2]]) -> AnalysisSnapshot {
        let mut peaks = [0.0_f32; 2];
        let mut sums = [0.0_f64; 2];
        let mut cross = 0.0_f64;
        let mut left_energy = 0.0_f64;
        let mut right_energy = 0.0_f64;

        for frame in frames {
            for channel in 0..2 {
                let sample = frame[channel];
                peaks[channel] = peaks[channel].max(sample.abs());
                sums[channel] += f64::from(sample) * f64::from(sample);
            }
            cross += f64::from(frame[0]) * f64::from(frame[1]);
            left_energy += f64::from(frame[0]) * f64::from(frame[0]);
            right_energy += f64::from(frame[1]) * f64::from(frame[1]);
        }

        self.fft_input.fill(0.0);
        let offset = frames.len().saturating_sub(self.fft_size);
        for (index, frame) in frames[offset..].iter().take(self.fft_size).enumerate() {
            self.fft_input[index] = (frame[0] + frame[1]) * 0.5 * self.window[index];
        }
        self.fft
            .process(&mut self.fft_input, &mut self.fft_output)
            .expect("preallocated FFT buffers have the planned sizes");

        let normalization = 2.0 / self.fft_size as f32;
        let spectrum_db = self
            .fft_output
            .iter()
            .map(|bin| amplitude_db(bin.norm() * normalization))
            .collect();
        let count = frames.len().max(1) as f64;
        let rms = [
            (sums[0] / count).sqrt() as f32,
            (sums[1] / count).sqrt() as f32,
        ];
        let loudness_power = ((sums[0] + sums[1]) / (2.0 * count)).max(1.0e-12);
        let correlation_denominator = (left_energy * right_energy).sqrt();
        let correlation = if correlation_denominator > 1.0e-12 {
            (cross / correlation_denominator).clamp(-1.0, 1.0) as f32
        } else {
            0.0
        };

        AnalysisSnapshot {
            spectrum_db,
            peak_dbfs: [amplitude_db(peaks[0]), amplitude_db(peaks[1])],
            rms_dbfs: [amplitude_db(rms[0]), amplitude_db(rms[1])],
            // BS.1770's absolute calibration offset. Full K-weighting and
            // gating belong to the long-running analysis worker milestone.
            momentary_lufs: (-0.691 + 10.0 * loudness_power.log10()) as f32,
            stereo_correlation: correlation,
        }
    }
}

fn amplitude_db(amplitude: f32) -> f32 {
    (20.0 * amplitude.max(1.0e-6).log10()).max(-120.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_channels_report_positive_correlation() {
        let mut analyzer = Analyzer::new(48_000.0, 1024);
        let frames: Vec<_> = (0..1024)
            .map(|index| {
                let sample = (std::f32::consts::TAU * 1000.0 * index as f32 / 48_000.0).sin();
                [sample, sample]
            })
            .collect();
        let snapshot = analyzer.analyze(&frames);
        assert!(snapshot.stereo_correlation > 0.999);
        assert!(snapshot.peak_dbfs[0] > -0.1);
    }

    #[test]
    fn opposed_channels_report_negative_correlation() {
        let mut analyzer = Analyzer::new(48_000.0, 1024);
        let frames: Vec<_> = (0..1024)
            .map(|index| {
                let sample = (std::f32::consts::TAU * index as f32 / 64.0).sin();
                [sample, -sample]
            })
            .collect();
        assert!(analyzer.analyze(&frames).stereo_correlation < -0.999);
    }
}

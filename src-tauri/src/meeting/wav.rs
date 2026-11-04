//! Streaming mono WAV writer for meeting recordings.
//!
//! Unlike `audio_toolkit::audio::utils::save_wav_file`, which needs the whole
//! recording in memory, this writes samples as they arrive. Audio goes to
//! `<name>.partial` and is renamed to `<name>` on `finish`, so a file without
//! the `.partial` suffix is always a complete recording.

use anyhow::{bail, Context, Result};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

pub struct MonoWavWriter {
    writer: WavWriter<BufWriter<File>>,
    partial_path: PathBuf,
    final_path: PathBuf,
    frames: u64,
}

impl MonoWavWriter {
    /// Creates `<final_path>.partial` and writes 16-bit mono PCM at `sample_rate`.
    pub fn create(final_path: &Path, sample_rate: u32) -> Result<Self> {
        if sample_rate == 0 {
            bail!("WAV sample rate must be non-zero");
        }
        let partial_path = PathBuf::from(format!("{}.partial", final_path.display()));
        let spec = WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let writer = WavWriter::create(&partial_path, spec)
            .with_context(|| format!("Creating {}", partial_path.display()))?;
        Ok(Self {
            writer,
            partial_path,
            final_path: final_path.to_path_buf(),
            frames: 0,
        })
    }

    /// Appends interleaved `samples` with `channels` channels, averaging them
    /// down to mono. `samples.len()` must be a multiple of `channels`.
    pub fn write_interleaved(&mut self, samples: &[f32], channels: usize) -> Result<()> {
        if channels == 0 || samples.len() % channels != 0 {
            bail!(
                "{} samples do not split into {channels} channels",
                samples.len()
            );
        }
        for frame in samples.chunks_exact(channels) {
            let mono = frame.iter().sum::<f32>() / channels as f32;
            self.writer
                .write_sample(to_i16(mono))
                .with_context(|| format!("Writing {}", self.partial_path.display()))?;
            self.frames += 1;
        }
        Ok(())
    }

    /// Frames written so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Finalizes the WAV header and renames the file to its final name.
    /// Returns the number of frames written.
    pub fn finish(self) -> Result<u64> {
        let Self {
            writer,
            partial_path,
            final_path,
            frames,
        } = self;
        writer
            .finalize()
            .with_context(|| format!("Finalizing {}", partial_path.display()))?;
        fs::rename(&partial_path, &final_path)
            .with_context(|| format!("Renaming to {}", final_path.display()))?;
        Ok(frames)
    }
}

fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("handy-wav-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn finish_renames_and_writes_readable_mono_wav() {
        let final_path = temp_path("finish.wav");
        let _ = fs::remove_file(&final_path);

        let mut writer = MonoWavWriter::create(&final_path, 16_000).unwrap();
        // Stereo frames: left 1.0, right 0.0 average to 0.5.
        writer
            .write_interleaved(&[1.0, 0.0, 1.0, 0.0], 2)
            .unwrap();
        assert_eq!(writer.frames(), 2);
        assert_eq!(writer.finish().unwrap(), 2);

        assert!(final_path.exists());
        assert!(!PathBuf::from(format!("{}.partial", final_path.display())).exists());

        let reader = hound::WavReader::open(&final_path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 16_000);
        let samples: Vec<i16> = reader.into_samples().map(|s| s.unwrap()).collect();
        assert_eq!(samples.len(), 2);
        assert!((samples[0] - (i16::MAX / 2)).abs() <= 1);
    }

    #[test]
    fn rejects_samples_that_do_not_fit_the_channel_count() {
        let final_path = temp_path("bad-channels.wav");
        let mut writer = MonoWavWriter::create(&final_path, 48_000).unwrap();
        assert!(writer.write_interleaved(&[0.0, 0.0, 0.0], 2).is_err());
        drop(writer);
        let _ = fs::remove_file(format!("{}.partial", final_path.display()));
    }

    #[test]
    fn clamps_out_of_range_samples() {
        assert_eq!(to_i16(2.0), i16::MAX);
        assert_eq!(to_i16(-2.0), -i16::MAX);
        assert_eq!(to_i16(0.0), 0);
    }
}

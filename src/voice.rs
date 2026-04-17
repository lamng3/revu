use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

pub struct Recorder {
    stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
    wav_path: PathBuf,
}

impl Recorder {
    pub fn start(wav_path: PathBuf) -> Result<Self> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| anyhow!("no input device"))?;
        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();
        let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let buf = samples.clone();

        let err_fn = |e| eprintln!("input stream error: {e}");

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    if let Ok(mut v) = buf.lock() {
                        v.extend_from_slice(data);
                    }
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _| {
                    if let Ok(mut v) = buf.lock() {
                        v.extend(data.iter().map(|s| *s as f32 / i16::MAX as f32));
                    }
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::U16 => device.build_input_stream(
                &config.into(),
                move |data: &[u16], _| {
                    if let Ok(mut v) = buf.lock() {
                        v.extend(data.iter().map(|s| (*s as f32 - 32768.0) / 32768.0));
                    }
                },
                err_fn,
                None,
            )?,
            _ => return Err(anyhow!("unsupported sample format")),
        };
        stream.play()?;
        Ok(Self { stream, samples, sample_rate, channels, wav_path })
    }

    pub fn stop(self) -> Result<PathBuf> {
        drop(self.stream);
        let samples = self.samples.lock().unwrap().clone();
        let mono: Vec<i16> = if self.channels > 1 {
            samples
                .chunks(self.channels as usize)
                .map(|c| {
                    let avg: f32 = c.iter().sum::<f32>() / c.len() as f32;
                    (avg.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
                })
                .collect()
        } else {
            samples.iter().map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).collect()
        };
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&self.wav_path, spec)?;
        for s in mono {
            w.write_sample(s)?;
        }
        w.finalize()?;
        // If sample rate != 16000, whisper-cli handles resampling itself.
        Ok(self.wav_path)
    }
}

pub fn model_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".revu")
        .join("models")
        .join("ggml-base.en.bin")
}

pub fn whisper_cli() -> Option<String> {
    for name in ["whisper-cli", "whisper-cpp", "main"] {
        if which(name).is_some() {
            return Some(name.to_string());
        }
    }
    None
}

fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(bin);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

pub fn ensure_model() -> Result<PathBuf> {
    let p = model_path();
    if p.exists() {
        return Ok(p);
    }
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    let url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";
    let out = Command::new("curl").args(["-L", "--fail", "-o", p.to_str().unwrap(), url]).output()?;
    if !out.status.success() {
        return Err(anyhow!("model download failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(p)
}

pub fn transcribe(wav: &Path) -> Result<String> {
    let Some(bin) = whisper_cli() else {
        return Err(anyhow!("whisper-cli not on PATH. install via `brew install whisper-cpp`."));
    };
    let model = ensure_model()?;
    let out = Command::new(&bin)
        .args([
            "-m", model.to_str().unwrap(),
            "-f", wav.to_str().unwrap(),
            "-nt",      // no timestamps
            "-otxt",    // write .txt sidecar
        ])
        .output()?;
    if !out.status.success() {
        return Err(anyhow!("whisper failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let txt_path = wav.with_extension("wav.txt");
    if txt_path.exists() {
        return Ok(fs::read_to_string(&txt_path)?.trim().to_string());
    }
    // Fall back to stdout
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

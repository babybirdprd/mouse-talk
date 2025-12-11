use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use anyhow::anyhow;

pub struct AudioRecorder {
    _stream: cpal::Stream,
}

impl AudioRecorder {
    pub fn new(sender: mpsc::Sender<Vec<f32>>) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No input device found"))?;

        let config = device.default_input_config()?;
        let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

        // We ideally want 16kHz mono.
        // sherpa-rs/sherpa-onnx usually expects 16kHz.
        // If the device supports it, great. If not, we might need to resample.
        // For this scaffold, we will implement a simple check and basic resampling if needed.

        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        println!("Input device sample rate: {}, channels: {}", sample_rate, channels);

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &_| {
                    process_audio_f32(data, sample_rate, channels, &sender);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &_| {
                    let data_f32: Vec<f32> = data.iter().map(|&x| x as f32 / i16::MAX as f32).collect();
                    process_audio_f32(&data_f32, sample_rate, channels, &sender);
                },
                err_fn,
                None,
            )?,
             cpal::SampleFormat::U16 => device.build_input_stream(
                &config.into(),
                move |data: &[u16], _: &_| {
                    let data_f32: Vec<f32> = data.iter().map(|&x| (x as f32 - u16::MAX as f32 / 2.0) / (u16::MAX as f32 / 2.0)).collect();
                    process_audio_f32(&data_f32, sample_rate, channels, &sender);
                },
                err_fn,
                None,
            )?,
            _ => return Err(anyhow!("Unsupported sample format")),
        };

        stream.play()?;

        Ok(AudioRecorder { _stream: stream })
    }
}

fn process_audio_f32(data: &[f32], sample_rate: u32, channels: u16, sender: &mpsc::Sender<Vec<f32>>) {
    // If stereo, take the first channel.
    let mut mono_data = if channels > 1 {
        data.chunks(channels as usize).map(|chunk| chunk[0]).collect::<Vec<f32>>()
    } else {
        data.to_vec()
    };

    // Simple resampling to 16kHz if needed.
    // This is a naive implementation (decimation) just to ensure it runs.
    // A proper implementation should use a library like `rubato` or `samplerate`.
    // We assume sample_rate is higher than 16000.
    let target_rate = 16000;
    if sample_rate != target_rate {
        let ratio = sample_rate as f32 / target_rate as f32;
        let mut new_data = Vec::with_capacity((mono_data.len() as f32 / ratio) as usize);
        let mut i = 0.0;
        while (i as usize) < mono_data.len() {
            new_data.push(mono_data[i as usize]);
            i += ratio;
        }
        mono_data = new_data;
    }

    if let Err(e) = sender.send(mono_data) {
        eprintln!("Error sending audio data: {}", e);
    }
}

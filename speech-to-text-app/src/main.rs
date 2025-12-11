mod audio;
mod keyboard;

use anyhow::Result;
use clap::Parser;
use rdev::{listen, EventType, Button};
use sherpa_rs::transducer::{TransducerConfig, TransducerRecognizer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "models/tokens.txt")]
    tokens: String,

    #[arg(long, default_value = "models/encoder.int8.onnx")]
    encoder: String,

    #[arg(long, default_value = "models/decoder.int8.onnx")]
    decoder: String,

    #[arg(long, default_value = "models/joiner.int8.onnx")]
    joiner: String,

    /// Enable streaming mode (Simulated by repeated transcription, unlikely to be performant)
    #[arg(long)]
    streaming: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if !std::path::Path::new(&args.tokens).exists() {
        eprintln!("Warning: Model file '{}' not found. Ensure models are downloaded.", args.tokens);
    }

    let config = TransducerConfig {
        encoder: args.encoder,
        decoder: args.decoder,
        joiner: args.joiner,
        tokens: args.tokens,
        num_threads: 4,
        sample_rate: 16000,
        feature_dim: 80,
        model_type: "transducer".to_string(), // TDT is a type of transducer, sherpa-onnx should auto-detect or we use "transducer"
        ..Default::default()
    };

    // Initialize recognizer
    // We wrap it in a Mutex because we need to share it (though we only use it in one place really)
    // TransducerRecognizer new() can return Result.
    let recognizer = Arc::new(Mutex::new(TransducerRecognizer::new(config).map_err(|e| anyhow::anyhow!("{:?}", e))?));

    println!("Model loaded.");
    println!("Press Side Button (detected as Unknown(4) or Unknown(5) usually) to start recording.");

    // Audio channel
    let (tx, rx) = mpsc::channel::<Vec<f32>>();

    // State management
    let is_recording = Arc::new(AtomicBool::new(false));

    // Audio buffer
    let audio_buffer = Arc::new(Mutex::new(Vec::new()));

    let is_recording_audio = is_recording.clone();
    let audio_buffer_clone = audio_buffer.clone();

    // Start Audio Thread
    thread::spawn(move || {
        let _recorder = audio::AudioRecorder::new(tx).expect("Failed to initialize audio");
        for samples in rx {
            if is_recording_audio.load(Ordering::Relaxed) {
                let mut buffer = audio_buffer_clone.lock().unwrap();
                buffer.extend_from_slice(&samples);
            } else {
                // Clear buffer if not recording to save memory/state?
                // Or we clear it when we start recording.
            }
        }
    });

    let recognizer_transcribe = recognizer.clone();
    let is_rec = is_recording.clone();
    let audio_buffer_transcribe = audio_buffer.clone();

    if let Err(error) = listen(move |event| {
        match event.event_type {
            EventType::ButtonPress(Button::Unknown(4)) | EventType::ButtonPress(Button::Unknown(5)) => {
                let currently_recording = is_rec.load(Ordering::Relaxed);
                if !currently_recording {
                    println!("Starting recording...");
                    // Clear buffer
                    {
                        let mut buffer = audio_buffer_transcribe.lock().unwrap();
                        buffer.clear();
                    }
                    is_rec.store(true, Ordering::Relaxed);
                } else {
                    println!("Stopping recording...");
                    is_rec.store(false, Ordering::Relaxed);

                    // Give a small moment for last chunks
                    thread::sleep(Duration::from_millis(100));

                    let samples = {
                        let buffer = audio_buffer_transcribe.lock().unwrap();
                        buffer.clone()
                    };

                    println!("Captured {} samples. Transcribing...", samples.len());

                    if samples.is_empty() {
                        println!("No audio captured.");
                    } else {
                        // Transcribe
                        let mut rec = recognizer_transcribe.lock().unwrap();
                        let text = rec.transcribe(16000, &samples);
                        println!("Transcription: {}", text);

                        // Type it out
                        keyboard::type_text(&text);
                    }
                }
            }
             // For debugging which button is pressed
            EventType::ButtonPress(_b) => {
               // println!("Button pressed: {:?}", _b);
            }
            _ => (),
        }
    }) {
        eprintln!("Error: {:?}", error);
    }

    Ok(())
}

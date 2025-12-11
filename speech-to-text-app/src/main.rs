mod audio;
mod keyboard;

use anyhow::Result;
use clap::Parser;
use rdev::{listen, Button, EventType};
use sherpa_rs::nemo_ctc::{NemoCtcConfig, NemoCtcRecognizer};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Speech-to-text application using Razer mouse buttons
/// 
/// Default controls:
/// Button 4 (Side): Streaming mode
/// Button 5 (Side): Batch mode
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the ONNX model file (e.g., model.int8.onnx)
    #[arg(long, default_value = "models/model.int8.onnx")]
    model: String,

    /// Path to the tokens file
    #[arg(long, default_value = "models/tokens.txt")]
    tokens: String,

    /// Number of threads to use for inference
    #[arg(long, default_value_t = 4)]
    num_threads: i32,

    /// Enable debug output from the recognizer
    #[arg(long)]
    debug: bool,
    
    /// Mouse button ID for streaming mode (Default: 2 - Side Button)
    #[arg(long, default_value_t = 2)]
    streaming_btn: u8,

    /// Mouse button ID for batch mode (Default: 1 - Side Button)
    #[arg(long, default_value_t = 1)]
    batch_btn: u8,
}

/// Recording state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum RecordingState {
    Idle = 0,
    BatchRecording = 1,
    StreamingRecording = 2,
}

impl From<u8> for RecordingState {
    fn from(v: u8) -> Self {
        match v {
            1 => RecordingState::BatchRecording,
            2 => RecordingState::StreamingRecording,
            _ => RecordingState::Idle,
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Check model files exist
    if !std::path::Path::new(&args.model).exists() {
        eprintln!("Error: Model file '{}' not found.", args.model);
        eprintln!("Download the Parakeet TDT-CTC 110M model:");
        eprintln!("  .\\download_model.ps1");
        return Err(anyhow::anyhow!("Model file not found"));
    }

    if !std::path::Path::new(&args.tokens).exists() {
        eprintln!("Error: Tokens file '{}' not found.", args.tokens);
        return Err(anyhow::anyhow!("Tokens file not found"));
    }

    println!("Loading NeMo CTC model...");
    let config = NemoCtcConfig {
        model: args.model,
        tokens: args.tokens,
        num_threads: Some(args.num_threads),
        debug: args.debug,
        ..Default::default()
    };

    let recognizer = Arc::new(Mutex::new(
        NemoCtcRecognizer::new(config).map_err(|e| anyhow::anyhow!("{:?}", e))?
    ));

    println!("✅ Model loaded successfully!");
    println!();
    println!("Controls:");
    println!("  Mouse Button {} (side): Streaming mode - Live transcription", args.streaming_btn);
    println!("  Mouse Button {} (side): Batch mode - Press to start, press again to transcribe", args.batch_btn);
    println!();
    println!("Waiting for input...");

    // Audio channel
    let (tx, rx) = mpsc::channel::<Vec<f32>>();

    // State management
    let recording_state = Arc::new(AtomicU8::new(RecordingState::Idle as u8));
    
    // Audio buffer for batch mode
    let audio_buffer = Arc::new(Mutex::new(Vec::new()));

    // Clone for audio thread
    let recording_state_audio = recording_state.clone();
    let audio_buffer_clone = audio_buffer.clone();
    let recognizer_streaming = recognizer.clone();

    // Start Audio Thread
    thread::spawn(move || {
        let _recorder = match audio::AudioRecorder::new(tx) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to initialize audio: {}", e);
                return;
            }
        };
        
        for samples in rx {
            let state = RecordingState::from(recording_state_audio.load(Ordering::Relaxed));
            
            match state {
                RecordingState::BatchRecording => {
                    // Accumulate samples for batch transcription
                    let mut buffer = audio_buffer_clone.lock().unwrap();
                    buffer.extend_from_slice(&samples);
                }
                RecordingState::StreamingRecording => {
                    // Accumulate for streaming, and periodically transcribe
                    let mut buffer = audio_buffer_clone.lock().unwrap();
                    buffer.extend_from_slice(&samples);
                    
                    // Transcribe every ~1.5 seconds of audio (24000 samples at 16kHz)
                    if buffer.len() >= 24000 {
                        let samples_to_transcribe = buffer.clone();
                        drop(buffer);
                        
                        if let Ok(mut rec) = recognizer_streaming.lock() {
                            let result = rec.transcribe(16000, &samples_to_transcribe);
                            if !result.text.trim().is_empty() {
                                print!("\r\x1b[K🎤 {}", result.text);
                                use std::io::Write;
                                std::io::stdout().flush().ok();
                            }
                        }
                    }
                }
                RecordingState::Idle => {
                    // Not recording, ignore samples
                }
            }
        }
    });

    let recognizer_main = recognizer.clone();
    let recording_state_main = recording_state.clone();
    let audio_buffer_main = audio_buffer.clone();

    // Capture args for closure
    let batch_btn = args.batch_btn;
    let streaming_btn = args.streaming_btn;

    if let Err(error) = listen(move |event| {
        // Simplify matching
        if let EventType::ButtonPress(button) = event.event_type {
            // Normalize button to u8 if possible for comparison
            let btn_id = match button {
                Button::Unknown(b) => Some(b),
                Button::Left => None, // Ignore standard left click to distinguish from Unknown(1)
                Button::Right => None, // Ignore standard right click to distinguish from Unknown(2)
                Button::Middle => Some(3),
            };

            if let Some(id) = btn_id {
                if id == batch_btn {
                    // Batch Mode Logic
                    let current_state = RecordingState::from(recording_state_main.load(Ordering::Relaxed));
                    match current_state {
                        RecordingState::Idle => {
                            println!("\n🔴 Batch recording started... (press Button {} again to stop)", batch_btn);
                            {
                                let mut buffer = audio_buffer_main.lock().unwrap();
                                buffer.clear();
                            }
                            recording_state_main.store(RecordingState::BatchRecording as u8, Ordering::Relaxed);
                        }
                        RecordingState::BatchRecording => {
                            println!("\n⏹️ Stopping batch recording...");
                            recording_state_main.store(RecordingState::Idle as u8, Ordering::Relaxed);
                            thread::sleep(Duration::from_millis(100));
                            let samples = {
                                let buffer = audio_buffer_main.lock().unwrap();
                                buffer.clone()
                            };
                            println!("📝 Transcribing {} samples...", samples.len());
                            if samples.is_empty() {
                                println!("⚠️ No audio captured.");
                            } else {
                                if let Ok(mut rec) = recognizer_main.lock() {
                                    let result = rec.transcribe(16000, &samples);
                                    println!("✅ Transcription: {}", result.text);
                                    if !result.text.trim().is_empty() {
                                        keyboard::type_text(&result.text);
                                    }
                                }
                            }
                        }
                        RecordingState::StreamingRecording => {
                            println!("\n⚠️ Currently in streaming mode. Stop that first.");
                        }
                    }
                } else if id == streaming_btn {
                    // Streaming Mode Logic
                    let current_state = RecordingState::from(recording_state_main.load(Ordering::Relaxed));
                    match current_state {
                        RecordingState::Idle => {
                            println!("\n🎤 Streaming mode started... (press Button {} again to stop)", streaming_btn);
                            {
                                let mut buffer = audio_buffer_main.lock().unwrap();
                                buffer.clear();
                            }
                            recording_state_main.store(RecordingState::StreamingRecording as u8, Ordering::Relaxed);
                        }
                        RecordingState::StreamingRecording => {
                            println!("\n⏹️ Stopping streaming...");
                            recording_state_main.store(RecordingState::Idle as u8, Ordering::Relaxed);
                            thread::sleep(Duration::from_millis(200));
                            let samples = {
                                let buffer = audio_buffer_main.lock().unwrap();
                                buffer.clone()
                            };
                            if !samples.is_empty() {
                                if let Ok(mut rec) = recognizer_main.lock() {
                                    let result = rec.transcribe(16000, &samples);
                                    println!("\n✅ Final transcription: {}", result.text);
                                    if !result.text.trim().is_empty() {
                                        keyboard::type_text(&result.text);
                                    }
                                }
                            }
                        }
                        RecordingState::BatchRecording => {
                            println!("\n⚠️ Currently in batch mode. Stop that first.");
                        }
                    }
                } else {
                    // Debug print for other buttons
                     println!("🔍 Button pressed: {:?} (ID: {}) Name: {:?}", button, id, event.name);
                }
            } else {
                 println!("🔍 Button pressed: {:?} Name: {:?}", button, event.name);
            }
        }
    }) {
        eprintln!("Error: {:?}", error);
    }

    Ok(())
}

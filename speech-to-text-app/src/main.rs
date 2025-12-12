mod audio;
mod keyboard;
mod tray;

use anyhow::Result;
use clap::Parser;
use muda::MenuEvent;
use rdev::{listen, Button, EventType};
use sherpa_rs::nemo_ctc::{NemoCtcConfig, NemoCtcRecognizer};
use sherpa_rs::punctuate::{Punctuation, PunctuationConfig};
use sherpa_rs::silero_vad::{SileroVad, SileroVadConfig};
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

    /// Show console window instead of running in background
    #[arg(long)]
    show_console: bool,

    /// Path to the Silero VAD model file
    #[arg(long, default_value = "models/silero_vad.onnx")]
    vad_model: String,

    /// Enable automatic punctuation
    #[arg(long)]
    punctuate: bool,

    /// Path to the punctuation model directory
    #[arg(long, default_value = "models/punct-ct-transformer/model.onnx")]
    punct_model: String,
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

    // Hide console unless --show-console is specified
    if !args.show_console {
        tray::hide_console_window();
    }

    // Initialize system tray
    let tray_app = tray::TrayApp::new()?;
    let streaming_enabled = tray_app.streaming_enabled.clone();
    let batch_enabled = tray_app.batch_enabled.clone();

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

    // Initialize VAD for streaming mode
    let vad_config = SileroVadConfig {
        model: args.vad_model.clone(),
        min_silence_duration: 0.5, // Wait 0.5s of silence before ending phrase
        min_speech_duration: 0.25, // Minimum speech to trigger
        max_speech_duration: 30.0, // Allow long speech segments
        threshold: 0.4, // Slightly more sensitive (lower = more sensitive)
        sample_rate: 16000,
        window_size: 512,
        ..Default::default()
    };
    let vad = Arc::new(Mutex::new(
        SileroVad::new(vad_config, 60.0).map_err(|e| anyhow::anyhow!("Failed to init VAD: {:?}", e))?
    ));
    println!("✅ VAD loaded successfully!");

    // Initialize punctuation if enabled
    let punctuator = if args.punctuate {
        if std::path::Path::new(&args.punct_model).exists() {
            let punct_config = PunctuationConfig {
                model: args.punct_model.clone(),
                ..Default::default()
            };
            match Punctuation::new(punct_config) {
                Ok(p) => {
                    println!("✅ Punctuation loaded successfully!");
                    Some(Arc::new(Mutex::new(p)))
                }
                Err(e) => {
                    eprintln!("Failed to load punctuation model: {:?}", e);
                    None
                }
            }
        } else {
            eprintln!("Punctuation model not found at '{}'. Run .\\download_model.ps1 to download.", args.punct_model);
            None
        }
    } else {
        None
    };

    println!();
    println!("Controls:");
    println!("  Mouse Button {} (side): Streaming mode - Live transcription", args.streaming_btn);
    println!("  Mouse Button {} (side): Batch mode - Press to start, press again to transcribe", args.batch_btn);
    if punctuator.is_some() {
        println!("  Auto-punctuation: ENABLED");
    }
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
    let vad_streaming = vad.clone();
    let punctuator_streaming = punctuator.clone();
    
    // Channel to send transcribed text for typing (streaming mode)
    let (text_tx, text_rx) = mpsc::channel::<String>();

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
                    // Use VAD to detect speech segments
                    if let Ok(mut vad) = vad_streaming.lock() {
                        vad.accept_waveform(samples.clone());
                        
                        // Check if we have complete speech segments
                        while !vad.is_empty() {
                            let segment = vad.front();
                            vad.pop();
                            
                            // Transcribe the speech segment
                            if !segment.samples.is_empty() {
                                if let Ok(mut rec) = recognizer_streaming.lock() {
                                    let result = rec.transcribe(16000, &segment.samples);
                                    let mut text = result.text.trim().to_string();
                                    
                                    // Apply punctuation if enabled
                                    if let Some(ref punct) = punctuator_streaming {
                                        if let Ok(mut p) = punct.lock() {
                                            text = p.add_punctuation(&text);
                                        }
                                    }
                                    
                                    if !text.is_empty() {
                                        println!("🎤 {}", text);
                                        let _ = text_tx.send(text);
                                    }
                                }
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
    let vad_main = vad.clone();
    let punctuator_main = punctuator.clone();

    // Thread to type transcribed text from streaming mode
    thread::spawn(move || {
        for text in text_rx {
            keyboard::type_text(&text);
            keyboard::type_text(" "); // Add space between chunks
        }
    });

    // Capture args for closure
    let batch_btn = args.batch_btn;
    let streaming_btn = args.streaming_btn;

    // Clone enabled flags for input handler
    let streaming_enabled_input = streaming_enabled.clone();
    let batch_enabled_input = batch_enabled.clone();

    // Start input listener in separate thread
    thread::spawn(move || {
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
                if id == batch_btn && batch_enabled_input.load(Ordering::Relaxed) {
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
                                    let mut text = result.text.trim().to_string();
                                    
                                    // Apply punctuation if enabled
                                    if let Some(ref punct) = punctuator_main {
                                        if let Ok(mut p) = punct.lock() {
                                            text = p.add_punctuation(&text);
                                        }
                                    }
                                    
                                    println!("✅ Transcription: {}", text);
                                    if !text.is_empty() {
                                        keyboard::type_text(&text);
                                    }
                                }
                            }
                        }
                        RecordingState::StreamingRecording => {
                            println!("\n⚠️ Currently in streaming mode. Stop that first.");
                        }
                    }
                } else if id == streaming_btn && streaming_enabled_input.load(Ordering::Relaxed) {
                    // Streaming Mode Logic
                    let current_state = RecordingState::from(recording_state_main.load(Ordering::Relaxed));
                    match current_state {
                        RecordingState::Idle => {
                            println!("\n🎤 Streaming mode started... (press Button {} again to stop)", streaming_btn);
                            // Clear VAD state for fresh start
                            if let Ok(mut v) = vad_main.lock() {
                                v.clear();
                            }
                            recording_state_main.store(RecordingState::StreamingRecording as u8, Ordering::Relaxed);
                        }
                        RecordingState::StreamingRecording => {
                            println!("\n⏹️ Stopping streaming...");
                            recording_state_main.store(RecordingState::Idle as u8, Ordering::Relaxed);
                            // Flush VAD to get any remaining speech
                            if let Ok(mut v) = vad_main.lock() {
                                v.flush();
                            }
                            println!("✅ Streaming complete");
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
    });

    // Main event loop for tray menu
    let menu_channel = MenuEvent::receiver();
    loop {
        if let Ok(event) = menu_channel.recv() {
            if tray_app.handle_menu_event(&event) {
                // Quit was selected
                break;
            }
        }
    }

    Ok(())
}

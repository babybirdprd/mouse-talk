mod audio;
mod keyboard;
mod overlay;
mod tray;

use anyhow::Result;
use clap::Parser;
use muda::MenuEvent;
use rdev::{listen, Button, EventType};
use sherpa_rs::nemo_ctc::{NemoCtcConfig, NemoCtcRecognizer};
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

    /// Mouse button ID for model loading toggle (Default: 1 - Side Button)
    #[arg(long, default_value_t = 1)]
    model_btn: u8,

    /// Show console window instead of running in background
    #[arg(long)]
    show_console: bool,

    /// Path to the Silero VAD model file
    #[arg(long, default_value = "models/silero_vad.onnx")]
    vad_model: String,
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

    // Resolve paths relative to executable location if not found in current dir
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
    
    let resolve_path = |path: &str| -> std::path::PathBuf {
        let p = std::path::Path::new(path);
        if p.exists() {
            return p.to_path_buf();
        }
        // Try next to executable
        let p_exe = exe_dir.join(path);
        if p_exe.exists() {
            return p_exe;
        }
        // Try up one level (useful for target/release/ usage)
        let p_up = exe_dir.join("..").join(path);
        if p_up.exists() {
            return p_up;
        }
        // Try up two levels (standard cargo run --release structure: target/release/ -> root)
        let p_up2 = exe_dir.join("../..").join(path);
        if p_up2.exists() {
            return p_up2;
        }
        // Return original to let it fail naturally or use as fallback
        p.to_path_buf()
    };

    let model_path = resolve_path(&args.model);
    let tokens_path = resolve_path(&args.tokens);
    let vad_path = resolve_path(&args.vad_model);

    // Initial check unnecessary for lazy loaded main model, but critical for VAD
    // We won't block main model, but we will warn if VAD is missing since it loads immediately
    if !vad_path.exists() {
        let msg = format!("VAD Model not found at: {}\nPlease check your models folder.", vad_path.display());
        show_error_box("Missing VAD Model", &msg);
        return Err(anyhow::anyhow!("VAD model not found"));
    }

    // Initialize recognizer container (empty at start)
    let recognizer: Arc<Mutex<Option<NemoCtcRecognizer>>> = Arc::new(Mutex::new(None));

    println!("ℹ️ Application started. Press Button {} to load models.", args.model_btn);

    // Initialize VAD for streaming mode
    let vad_config = SileroVadConfig {
        model: vad_path.to_string_lossy().to_string(),
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

    // Initialize overlay controller for live transcription preview
    let overlay = Arc::new(overlay::OverlayController::new());

    println!();
    println!("Controls:");
    println!("  Mouse Button {} (side): Streaming mode - Live transcription", args.streaming_btn);
    println!("  Mouse Button {} (side): Toggle Models - Load/Unload ASR models", args.model_btn);
    println!();
    println!("Waiting for input...");

    // Audio channel
    let (tx, rx) = mpsc::channel::<Vec<f32>>();

    // State management
    let recording_state = Arc::new(AtomicU8::new(RecordingState::Idle as u8));
    
    // Audio buffer for batch mode
    let audio_buffer = Arc::new(Mutex::new(Vec::new()));
    
    // Audio buffer for streaming mode (accumulate ALL audio for final re-transcription)
    let streaming_audio_buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    
    // Accumulated text for overlay display
    let streaming_text_display = Arc::new(Mutex::new(String::new()));

    // Clone for audio thread
    let recording_state_audio = recording_state.clone();
    let audio_buffer_clone = audio_buffer.clone();
    let recognizer_streaming = recognizer.clone();
    let vad_streaming = vad.clone();
    let streaming_audio_clone = streaming_audio_buffer.clone();
    
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
                    // Accumulate ALL audio for final re-transcription
                    if let Ok(mut buf) = streaming_audio_clone.lock() {
                        buf.extend_from_slice(&samples);
                    }
                    
                    // Use VAD to detect speech segments for live preview
                    if let Ok(mut vad) = vad_streaming.lock() {
                        vad.accept_waveform(samples.clone());
                        
                        // Check if we have complete speech segments
                        while !vad.is_empty() {
                            let segment = vad.front();
                            vad.pop();
                            
                            // Transcribe the speech segment (raw, no punctuation)
                            if !segment.samples.is_empty() {
                                if let Ok(mut rec_opt) = recognizer_streaming.lock() {
                                    if let Some(rec) = rec_opt.as_mut() {
                                        let result = rec.transcribe(16000, &segment.samples);
                                        let text = result.text.trim().to_string();
                                        
                                        if !text.is_empty() {
                                            println!("🎤 {}", text);
                                            let _ = text_tx.send(text);
                                        }
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
    let streaming_audio_main = streaming_audio_buffer.clone();
    let overlay_main = overlay.clone();

    // Thread to update overlay with transcribed text (streaming mode)
    let overlay_typing = overlay.clone();
    thread::spawn(move || {
        for text in text_rx {
            // Append text to overlay display
            overlay_typing.append_text(&text);
        }
    });

    // Capture args for closure
    let model_btn = args.model_btn;
    let streaming_btn = args.streaming_btn;
    
    // Capture config strings for loading thread (use resolved paths)
    let model_path = model_path.to_string_lossy().to_string();
    let tokens_path = tokens_path.to_string_lossy().to_string();
    let num_threads = args.num_threads;
    let debug_mode = args.debug;

    // Clone enabled flags for input handler
    let streaming_enabled_input = streaming_enabled.clone();
    let model_btn_enabled_input = batch_enabled.clone(); // Reusing batch toggle for model toggle

    // Start input listener in separate thread
    thread::spawn(move || {
        if let Err(error) = listen(move |event| {
        // Simplify matching
        if let EventType::ButtonPress(button) = event.event_type {
            // Normalize button to u8 if possible for comparison
            let btn_id = match button {
                Button::Unknown(b) => Some(b),
                Button::Left => None, 
                Button::Right => None,
                Button::Middle => Some(3),
            };

            if let Some(id) = btn_id {
                if id == model_btn && model_btn_enabled_input.load(Ordering::Relaxed) {
                    // Model Toggle Logic
                    let mut rec_guard = recognizer_main.lock().unwrap();
                    
                    if rec_guard.is_some() {
                        // Unload
                        *rec_guard = None;
                        println!("📦 Models unloaded");
                        
                        overlay_main.show();
                        overlay_main.set_text("Models Unloaded");
                        thread::spawn({
                            let ov = overlay_main.clone();
                            move || {
                                thread::sleep(Duration::from_secs(1));
                                ov.hide();
                            }
                        });
                    } else {
                        // Load
                        println!("⏳ Loading models...");
                        overlay_main.show();
                        overlay_main.set_text("Loading Models...");
                        
                        let rec_clone = recognizer_main.clone();
                        let ov = overlay_main.clone();
                        let m_path = model_path.clone();
                        let t_path = tokens_path.clone();
                        
                        thread::spawn(move || {
                             if !std::path::Path::new(&m_path).exists() {
                                ov.set_text("Error: Model file not found");
                                thread::sleep(Duration::from_secs(2));
                                ov.hide();
                                return;
                            }

                            let config = NemoCtcConfig {
                                model: m_path,
                                tokens: t_path,
                                num_threads: Some(num_threads),
                                debug: debug_mode,
                                ..Default::default()
                            };

                            match NemoCtcRecognizer::new(config) {
                                Ok(rec) => {
                                    {
                                        let mut g = rec_clone.lock().unwrap();
                                        *g = Some(rec);
                                    }
                                    println!("✅ Models loaded!");
                                    ov.set_text("Models Loaded");
                                    thread::sleep(Duration::from_secs(1));
                                    ov.hide();
                                }
                                Err(e) => {
                                    eprintln!("Failed to load: {}", e);
                                    ov.set_text("Failed to load models");
                                    thread::sleep(Duration::from_secs(2));
                                    ov.hide();
                                }
                            }
                        });
                    }
                } else if id == streaming_btn && streaming_enabled_input.load(Ordering::Relaxed) {
                    // Streaming Mode Logic
                    
                    // Check if models are loaded checks
                    let models_loaded = {
                         recognizer_main.lock().unwrap().is_some()
                    };
                    
                    if !models_loaded {
                        println!("⚠️ Models not loaded. Press Button {} to load.", model_btn);
                        overlay_main.show();
                        overlay_main.set_text("Models Not Loaded!");
                        thread::spawn({
                            let ov = overlay_main.clone();
                            move || {
                                thread::sleep(Duration::from_secs(1));
                                ov.hide();
                            }
                        });
                    } else {
                        let current_state = RecordingState::from(recording_state_main.load(Ordering::Relaxed));
                        match current_state {
                            RecordingState::Idle => {
                                println!("\n🎤 Streaming mode started... (press Button {} again to stop)", streaming_btn);
                                // Clear VAD and audio buffer
                                if let Ok(mut v) = vad_main.lock() {
                                    v.clear();
                                }
                                if let Ok(mut buf) = streaming_audio_main.lock() {
                                    buf.clear();
                                }
                                // Show overlay
                                overlay_main.show();
                                overlay_main.set_text("Listening..."); // Reset text
                                recording_state_main.store(RecordingState::StreamingRecording as u8, Ordering::Relaxed);
                            }
                            RecordingState::StreamingRecording => {
                                println!("\n⏹️ Stopping streaming...");
                                recording_state_main.store(RecordingState::Idle as u8, Ordering::Relaxed);
                                
                                // Hide overlay
                                overlay_main.hide();
                                
                                // Flush VAD to get any remaining speech
                                if let Ok(mut v) = vad_main.lock() {
                                    v.flush();
                                }
                                // Wait for last transcriptions to come through
                                thread::sleep(Duration::from_millis(400));
                                
                                // Get the accumulated audio
                                let audio_samples = {
                                    let buf = streaming_audio_main.lock().unwrap();
                                    buf.clone()
                                };
                                
                                if !audio_samples.is_empty() {
                                    println!("📝 Re-transcribing {} samples for proper punctuation...", audio_samples.len());
                                    
                                    // Re-transcribe ALL audio at once
                                    if let Ok(mut rec_opt) = recognizer_main.lock() {
                                        if let Some(rec) = rec_opt.as_mut() {
                                            let result = rec.transcribe(16000, &audio_samples);
                                            let final_text = result.text.trim();
                                            
                                            if !final_text.is_empty() {
                                                println!("✨ Final: {}", final_text);
                                                // Type the final, properly punctuated text
                                                keyboard::paste_text(final_text);
                                            }
                                        }
                                    }
                                }
                                
                                // Clear audio buffer
                                if let Ok(mut buf) = streaming_audio_main.lock() {
                                    buf.clear();
                                }
                                
                                println!("✅ Streaming complete");
                            }
                             _ => {}
                        }
                    }
                } else {
                    // Debug print for other buttons
                     // println!("🔍 Button pressed: {:?} (ID: {}) Name: {:?}", button, id, event.name);
                }
            } else {
                 // println!("🔍 Button pressed: {:?} Name: {:?}", button, event.name);
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

/// Helper to show a message box for errors (Windows only)
#[cfg(windows)]
fn show_error_box(title: &str, message: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winuser::{MessageBoxW, MB_OK, MB_ICONERROR};
    
    let wide_title: Vec<u16> = OsStr::new(title).encode_wide().chain(std::iter::once(0)).collect();
    let wide_message: Vec<u16> = OsStr::new(message).encode_wide().chain(std::iter::once(0)).collect();
    
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            wide_message.as_ptr(),
            wide_title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
fn show_error_box(title: &str, message: &str) {
    eprintln!("{}: {}", title, message);
}

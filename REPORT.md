# Code Audit Report

## 1. Executive Summary

This report details the findings of a full code audit of the `speech-to-text-app`, a Rust-based application utilizing `sherpa-rs` for speech recognition. The audit paid specific attention to the streaming mode implementation as requested.

**Key Findings:**
*   **Critical Performance Issue**: The streaming mode implementation performs a double transcription (once for preview, once for final text), which is inefficient and scales poorly.
*   **Blocking Input**: The final transcription logic in streaming mode runs inside the input hook callback, which can freeze system input.
*   **Audio Quality**: Naive audio resampling degrades recognition accuracy.
*   **Platform Lock-in**: The application is heavily coupled to Windows APIs.

## 2. Architecture Overview

The application is designed as a background utility that captures audio when specific mouse buttons are pressed.

*   **Core**: `main.rs` orchestrates the application, handling input, audio, and model inference.
*   **Audio**: `audio.rs` uses `cpal` to capture audio from the default input device.
*   **Input**: `rdev` is used to hook global mouse events to trigger recording.
*   **Inference**: `sherpa-rs` provides bindings to Sherpa-Onnx (using Silero VAD and NeMo CTC models).
*   **UI**: A custom Windows-native overlay (`overlay.rs`) provides visual feedback.

## 3. Streaming Mode Deep Dive

The streaming mode allows users to see transcription in real-time on an overlay and then pastes the final text when the button is released.

### Current Implementation Flow
1.  **Recording**: Audio is captured in a dedicated thread.
2.  **VAD**: Silero VAD processes audio chunks.
3.  **Preview Transcription**: When VAD detects a speech segment, it is immediately transcribed and sent to the overlay.
4.  **Buffering**: *Simultaneously*, all raw audio samples are appended to a `streaming_audio_buffer`.
5.  **Finalization**: When the mouse button is released, the application:
    *   Locks the `streaming_audio_buffer`.
    *   **Re-transcribes the entire buffer** using the same model to generate "properly punctuated" text.
    *   Pastes the result.

### Critical Issues

#### 3.1 Double Transcription & Inefficiency
The application transcribes the audio twice: once in small chunks for the UI, and again in full for the final output.
*   **Impact**: Wasted CPU/GPU cycles. For long recordings (e.g., 30s+), the final re-transcription adds a noticeable delay after the button is released.
*   **Root Cause**: The streaming transcription loop handles segments independently without context, while the final pass uses the full context.

#### 3.2 Blocking Input Hook
The logic to perform the final re-transcription is located inside the `rdev` input hook callback in `main.rs`.

```rust
// Inside rdev::grab callback
RecordingState::StreamingRecording => {
    // ...
    // Re-transcribe ALL audio at once
    if let Ok(mut rec_opt) = recognizer_main.lock() {
        // ...
        let result = rec.transcribe(16000, &audio_samples); // <--- BLOCKING CALL
        // ...
    }
}
```

*   **Impact**: `rdev` callbacks must be fast. Blocking this callback blocks the processing of *all* system input events (mouse clicks, key presses) on the thread handling the hook. If the model takes 2 seconds to transcribe, the user's mouse cursor and keyboard will likely freeze for 2 seconds.
*   **Recommendation**: The button release event should only signal a state change or send a message to a worker thread. The transcription must happen asynchronously.

#### 3.3 Mutex Contention
The `streaming_audio_buffer` is locked by the audio thread (high frequency) and the main input thread. While not currently causing deadlocks, it creates unnecessary contention.

## 4. Audio Pipeline

### 4.1 Naive Resampling
The `audio.rs` module implements resampling using simple decimation (skipping samples):

```rust
while (i as usize) < mono_data.len() {
    new_data.push(mono_data[i as usize]);
    i += ratio;
}
```

*   **Impact**: This introduces severe aliasing artifacts, making the audio sound "crunchy" or metallic to the model. This significantly degrades Speech-to-Text (STT) accuracy, especially in noisy environments or with high-pitched voices.
*   **Recommendation**: Use a proper resampling library like `rubato` or `samplerate` that implements a low-pass filter before downsampling.

## 5. Code Quality & Structure

*   **Monolithic `main.rs`**: The main file handles CLI parsing, tray management, model loading, application state, and input event logic. This makes it hard to test and maintain.
*   **Platform Coupling**: `overlay.rs` and parts of `keyboard.rs` use raw `winapi` calls. This makes porting to Linux/macOS difficult.
*   **Hardcoded Configuration**: Model paths, button IDs (mapped to generic "Side Button"), and audio parameters are either hardcoded or mixed with CLI args. A centralized `Config` struct would be better.
*   **Error Handling**: There are several `unwrap()` calls on Mutexes. If a thread panics while holding a lock, the entire application will crash on the next access (poisoned mutex).

## 6. Recommendations

### Immediate Fixes
1.  **Offload Final Transcription**: Move the final transcription logic out of the `rdev` callback. Use a channel to signal a worker thread to perform the transcription and paste operation.
2.  **Fix Resampling**: Replace the custom resampling loop with a crate like `samplerate` or `rubato`.

### Architectural Improvements
1.  **True Streaming**: Instead of re-transcribing everything, use a model or decoder that supports stateful streaming (e.g., maintain the decoder context between chunks). This would eliminate the need for the second pass.
2.  **Configuration Management**: Move all constants and CLI args into a configuration file (e.g., `config.toml`) or a dedicated settings module.
3.  **Cross-Platform Abstraction**: Create traits for `Overlay` and `Input` to allow for Linux/macOS implementations in the future.

### Refactoring
1.  Extract `App` state management into its own struct/module.
2.  Create a dedicated `TranscriptionService` that manages the model, VAD, and audio buffers, exposing a clean async or channel-based API.

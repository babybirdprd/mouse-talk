# Mouse-Talk

Speech-to-text transcription using Razer mouse buttons and NVIDIA Parakeet TDT-CTC 110M model.

## Features

- **Dual Mode Operation**:
  - **Button 4** (top side button): Streaming mode - Live transcription as you speak
  - **Button 5** (bottom side button): Batch mode - Record until button press, then transcribe
- Uses NVIDIA Parakeet TDT-CTC 110M model with int8 quantization (~110MB)
- Types transcribed text directly into any active text field
- Works with Razer Synapse running

## Quick Start

1. **Download the model**:
   ```powershell
   cd speech-to-text-app
   .\download_model.ps1
   ```

2. **Build and run**:
   ```powershell
   cargo run --release
   ```

3. **Use**:
   - Click in any text field
   - Press **Button 4** for streaming (live) transcription
   - Press **Button 5** to start batch recording, press again to transcribe

## Controls

| Button | Mode | Behavior |
|--------|------|----------|
| Button 4 (side) | Streaming | Live transcription while speaking, types on stop |
| Button 5 (side) | Batch | Start recording → Press again → Transcribe and type |

## Troubleshooting

### Button codes don't work
The app prints button codes when pressed. If your Razer mouse uses different codes, modify the `Button::Unknown(4)` and `Button::Unknown(5)` values in `main.rs`.

### Model not found
Run the download script or manually download from:
https://github.com/k2-fsa/sherpa-onnx/releases

## License

Uses [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) and [Parakeet TDT-CTC 110M](https://huggingface.co/nvidia/parakeet-tdt_ctc-110m) model (OpenRAIL-M license).
# Mouse-Talk

Speech-to-text transcription using Razer mouse buttons and NVIDIA Parakeet TDT-CTC 110M model.

## Features

- **Dual Mode Operation**:
  - **Button 2** (side): Streaming mode - Live transcription as you speak
  - **Button 1** (side): Batch mode - Record until button press, then transcribe
- Uses NVIDIA Parakeet TDT-CTC 110M model with int8 quantization (~110MB)
- Types transcribed text directly into any active text field
- System tray icon for background operation
- Works with Razer Synapse running

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (cargo)
- [CMake](https://cmake.org/download/)
- [Clang/LLVM](https://releases.llvm.org/download.html)
- Git

### Windows

For convenience on Windows:
```powershell
winget install -e --id Kitware.CMake
winget install -e --id LLVM.LLVM
```

## Quick Start

1. **Clone the repository** (with submodules):
   ```powershell
   git clone --recursive https://github.com/your-username/mouse-talk.git
   cd mouse-talk
   ```

   If you already cloned without `--recursive`:
   ```powershell
   git submodule update --init --recursive
   ```

2. **Download the model**:
   ```powershell
   cd speech-to-text-app
   .\download_model.ps1
   ```

3. **Build and run**:
   ```powershell
   cargo run --release
   ```

4. **Use**:
   - Click in any text field
   - Press **Button 2** for streaming (live) transcription
   - Press **Button 1** to start batch recording, press again to transcribe

## Controls

| Button | Mode | Behavior |
|--------|------|----------|
| Button 2 (side) | Streaming | Live transcription while speaking, types on stop |
| Button 1 (side) | Batch | Start recording → Press again → Transcribe and type |

Configure different buttons via CLI:
```powershell
cargo run --release -- --streaming-btn 4 --batch-btn 5
```

## System Tray

The application runs in the system tray. Right-click for options:
- Toggle streaming mode on/off
- Toggle batch mode on/off  
- Quit

## Build Notes

This project uses a fork of [sherpa-rs](https://github.com/thewh1teagle/sherpa-rs) which includes the [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) C bindings as a git submodule. The `--recursive` clone flag is required.

For detailed build instructions (CUDA, static linking, etc.), see [sherpa-rs/BUILDING.md](sherpa-rs/BUILDING.md).

## Troubleshooting

### Button codes don't work
The app prints button codes when pressed. If your mouse uses different codes, use the `--streaming-btn` and `--batch-btn` CLI args.

### Model not found
Run the download script or manually download from:
https://github.com/k2-fsa/sherpa-onnx/releases

### Build errors
Ensure CMake and Clang are in your PATH. On Windows, restart your terminal after installing.

## License

Uses [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) and [Parakeet TDT-CTC 110M](https://huggingface.co/nvidia/parakeet-tdt_ctc-110m) model (OpenRAIL-M license).
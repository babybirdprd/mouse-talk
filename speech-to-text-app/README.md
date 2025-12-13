# Mouse-Talk

A local, low-latency speech-to-text application controlled by your mouse buttons. Uses NeMo CTC for speech recognition and Silero VAD for voice activity detection.

## Features

- **Local Processing**: All transcription happens on your device. No data leaves your computer.
- **Lazy Loading**: heavy AI models are only loaded when you need them.
- **Streaming Mode**: Live transcription as you speak.
- **Overlay**: Minimalist overlay showing your transcription status.

## Setup

1.  **Download Models**:
    Run the included PowerShell script to download the required models:
    ```powershell
    .\download_model.ps1
    ```

2.  **Run the App**:
    ```powershell
    cargo run --release
    ```

## Controls

- **Mouse Button 1 (Side)**: **Toggle Models**
    -   Press to load the AI models into memory.
    -   Press again to unload them (freeing up RAM).
- **Mouse Button 2 (Side)**: **Streaming Mode**
    -   Press to start listening.
    -   Speak naturally.
    -   Press again to stop. The final text will be typed into your active window.
    -   *Note: Models must be loaded first.*

## Running on Startup

To have Mouse-Talk start automatically with Windows:

1.  Build the release version:
    ```powershell
    cargo build --release
    ```
2.  Locate the executable in `target\release\speech-to-text-app.exe`.
3.  Press `Win + R`, type `shell:startup`, and press Enter. This opens your Startup folder.
4.  Right-click and drag the `speech-to-text-app.exe` into the Startup folder, then select **"Create shortcuts here"**.
5.  (Optional) Right-click the shortcut -> Properties -> Shortcut tab -> Run: "Minimized" (if you want it to start silently, though the app hides its console by default).

## Customization

You can change the mouse buttons by passing arguments:
```powershell
speech-to-text-app.exe --streaming-btn 4 --model-btn 5
```

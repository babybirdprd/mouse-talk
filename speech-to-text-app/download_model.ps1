# Mouse-Talk: Download Model Script
# Downloads the Parakeet TDT 110M model (Sherpa-ONNX format)

$modelsDir = "models"
$modelUrl = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet_tdt_ctc_110m-en-36000-int8.tar.bz2"
$archiveName = "sherpa-onnx-nemo-parakeet_tdt_ctc_110m-en-36000-int8.tar.bz2"
$extractedDir = "sherpa-onnx-nemo-parakeet_tdt_ctc_110m-en-36000-int8"

Write-Host "=========================================="
Write-Host "Mouse-Talk Model Downloader"
Write-Host "=========================================="
Write-Host ""

# Check if models already exist
if (Test-Path "$modelsDir\model.int8.onnx") {
    Write-Host "Model already exists in $modelsDir\"
    Write-Host "Delete the models folder to re-download."
    exit 0
}

# Create models directory
New-Item -ItemType Directory -Force -Path $modelsDir | Out-Null

Write-Host "Downloading Parakeet TDT 110M model..."
Write-Host "URL: $modelUrl"
Write-Host "This may take a few minutes..."
Write-Host ""

try {
    # Download archive
    $dest = "$modelsDir\$archiveName"
    
    # Use curl if available (faster, shows progress)
    $curlPath = (Get-Command curl.exe -ErrorAction SilentlyContinue)
    if ($curlPath) {
        & curl.exe -L -o $dest $modelUrl --progress-bar
    } else {
        Invoke-WebRequest -Uri $modelUrl -OutFile $dest -UseBasicParsing
    }
    
    Write-Host "  ✓ Archive downloaded"
    
    Write-Host "Extracting..."
    # Use tar to extract (Windows 10+ has tar)
    tar -xvf $dest -C $modelsDir
    
    # Move files to root of models dir
    Move-Item "$modelsDir\$extractedDir\*" "$modelsDir\" -Force
    
    # Clean up
    Remove-Item $dest
    Remove-Item "$modelsDir\$extractedDir" -Recurse -Force
    
    Write-Host "  ✓ Extracted successfully"
    
} catch {
    Write-Host "  ✗ Failed to download or extract"
    Write-Host "    Error: $_"
    exit 1
}

Write-Host ""
Write-Host "=========================================="
Write-Host "Model downloaded successfully!"
Write-Host "=========================================="
Write-Host ""

# Download VAD model
$vadUrl = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx"
$vadDest = "$modelsDir\silero_vad.onnx"

if (-not (Test-Path $vadDest)) {
    Write-Host "Downloading Silero VAD model..."
    try {
        $curlPath = (Get-Command curl.exe -ErrorAction SilentlyContinue)
        if ($curlPath) {
            & curl.exe -L -o $vadDest $vadUrl --progress-bar
        } else {
            Invoke-WebRequest -Uri $vadUrl -OutFile $vadDest -UseBasicParsing
        }
        Write-Host "  ✓ VAD model downloaded"
    } catch {
        Write-Host "  ✗ Failed to download VAD model: $_"
    }
} else {
    Write-Host "VAD model already exists."
}

# Download Punctuation model
$punctUrl = "https://github.com/k2-fsa/sherpa-onnx/releases/download/punctuation-models/sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12.tar.bz2"
$punctDest = "$modelsDir\punct-ct-transformer"
$punctModel = "$punctDest\model.onnx"

if (-not (Test-Path $punctModel)) {
    Write-Host "Downloading Punctuation model..."
    try {
        $curlPath = (Get-Command curl.exe -ErrorAction SilentlyContinue)
        if ($curlPath) {
            & curl.exe -L -o "$modelsDir\punct.tar.bz2" $punctUrl --progress-bar
        } else {
            Invoke-WebRequest -Uri $punctUrl -OutFile "$modelsDir\punct.tar.bz2" -UseBasicParsing
        }
        Push-Location $modelsDir
        tar -xf "punct.tar.bz2"
        Pop-Location
        if (Test-Path "$modelsDir\sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12") {
            Rename-Item "$modelsDir\sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12" $punctDest -Force
        }
        Remove-Item "$modelsDir\punct.tar.bz2" -Force -ErrorAction SilentlyContinue
        Write-Host "  ✓ Punctuation model downloaded"
    } catch {
        Write-Host "  ✗ Failed to download punctuation model: $_"
    }
} else {
    Write-Host "Punctuation model already exists."
}

Write-Host ""
Write-Host "Model files are in: $modelsDir\"
Write-Host "  - model.int8.onnx (ASR)"
Write-Host "  - tokens.txt"
Write-Host "  - silero_vad.onnx (VAD)"
Write-Host "  - punct-ct-transformer/model.onnx (Punctuation)"
Write-Host ""
Write-Host "You can now run the app with:"
Write-Host "  cargo run --release"
Write-Host ""
Write-Host "To enable auto-punctuation:"
Write-Host "  cargo run --release -- --punctuate"
Write-Host ""

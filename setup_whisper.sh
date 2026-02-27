#!/usr/bin/env bash
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WHISPER_DIR="$SCRIPT_DIR/whisper.cpp"

echo "==> Installing Vulkan dependencies..."
sudo apt-get install -y libvulkan-dev vulkan-tools

echo "==> Cloning whisper.cpp..."
if [ ! -d "$WHISPER_DIR" ]; then
    git clone https://github.com/ggerganov/whisper.cpp "$WHISPER_DIR"
else
    echo "    Already cloned, pulling latest..."
    git -C "$WHISPER_DIR" pull
fi

echo "==> Building with Vulkan support..."
cmake -B "$WHISPER_DIR/build" -S "$WHISPER_DIR" -DGGML_VULKAN=ON
cmake --build "$WHISPER_DIR/build" --config Release -j$(nproc)

echo "==> Downloading medium.en model..."
bash "$WHISPER_DIR/models/download-ggml-model.sh" medium.en

echo ""
echo "Done! Run: python main.py"

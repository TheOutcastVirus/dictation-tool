#!/usr/bin/env bash
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WHISPER_DIR="$SCRIPT_DIR/whisper.cpp"

echo "==> Installing LunarG Vulkan SDK (system libvulkan-dev is too old)..."
CODENAME=$(. /etc/os-release && echo "$UBUNTU_CODENAME")
wget -qO- https://packages.lunarg.com/lunarg-signing-key-pub.asc \
    | sudo tee /etc/apt/trusted.gpg.d/lunarg.asc > /dev/null
sudo wget -qO /etc/apt/sources.list.d/lunarg-vulkan.list \
    "https://packages.lunarg.com/vulkan/lunarg-vulkan-${CODENAME}.list"
sudo apt-get update -q
sudo apt-get install -y vulkan-sdk

echo "==> Cloning whisper.cpp..."
if [ ! -d "$WHISPER_DIR" ]; then
    git clone https://github.com/ggerganov/whisper.cpp "$WHISPER_DIR"
else
    echo "    Already cloned, pulling latest..."
    git -C "$WHISPER_DIR" pull
fi

echo "==> Building with Vulkan support..."
rm -rf "$WHISPER_DIR/build"
cmake -B "$WHISPER_DIR/build" -S "$WHISPER_DIR" -DGGML_VULKAN=ON
cmake --build "$WHISPER_DIR/build" --config Release -j$(nproc)

echo "==> Downloading medium.en model..."
bash "$WHISPER_DIR/models/download-ggml-model.sh" medium.en

echo ""
echo "Done! Run: python main.py"

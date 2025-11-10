#!/bin/bash
#
# GLiNER Environment Setup Script for macOS
# Fixes ONNX Runtime library loading issues on Mac
#

set -e

echo "=== GLiNER macOS Environment Setup ==="
echo

# Detect architecture
ARCH=$(uname -m)
if [ "$ARCH" = "arm64" ]; then
    ONNX_ARCH="arm64"
    echo "Detected: Apple Silicon (M1/M2/M3)"
elif [ "$ARCH" = "x86_64" ]; then
    ONNX_ARCH="x64"
    echo "Detected: Intel Mac"
else
    echo "❌ Unsupported architecture: $ARCH"
    exit 1
fi

# Step 1: Download ONNX Runtime
echo
echo "[1/5] Downloading ONNX Runtime for macOS ($ONNX_ARCH)..."
ONNX_VERSION="1.20.1"
ONNX_DIR="$HOME/.local/onnxruntime"

if [ ! -d "$ONNX_DIR" ]; then
    echo "  Downloading ONNX Runtime v${ONNX_VERSION}..."
    cd /tmp

    if [ "$ONNX_ARCH" = "arm64" ]; then
        # Apple Silicon
        ONNX_FILE="onnxruntime-osx-arm64-${ONNX_VERSION}.tgz"
    else
        # Intel Mac
        ONNX_FILE="onnxruntime-osx-x86_64-${ONNX_VERSION}.tgz"
    fi

    curl -L -o "$ONNX_FILE" \
        "https://github.com/microsoft/onnxruntime/releases/download/v${ONNX_VERSION}/${ONNX_FILE}"

    tar -xzf "$ONNX_FILE"

    echo "  Installing to $ONNX_DIR..."
    mkdir -p "$ONNX_DIR"

    # Extract directory name (handles both x86_64 and arm64 naming)
    EXTRACTED_DIR=$(tar -tzf "$ONNX_FILE" | head -1 | cut -f1 -d"/")
    cp -r "$EXTRACTED_DIR"/* "$ONNX_DIR/"

    echo "  Cleaning up..."
    rm -f "$ONNX_FILE"
    rm -rf "$EXTRACTED_DIR"
    cd - > /dev/null
else
    echo "  ONNX Runtime already installed at $ONNX_DIR"
fi

# Step 2: Set environment variables
echo
echo "[2/5] Setting environment variables..."
export ORT_STRATEGY=system
export ORT_LIB_LOCATION="$ONNX_DIR/lib"
export DYLD_LIBRARY_PATH="$ONNX_DIR/lib:$DYLD_LIBRARY_PATH"

echo "  ORT_STRATEGY=system"
echo "  ORT_LIB_LOCATION=$ONNX_DIR/lib"
echo "  DYLD_LIBRARY_PATH=$ONNX_DIR/lib:\$DYLD_LIBRARY_PATH"

# Step 3: Create environment file
echo
echo "[3/5] Creating .env.macos file..."
cat > .env.macos << EOF
# GLiNER Environment Variables for macOS
export ORT_STRATEGY=system
export ORT_LIB_LOCATION=$ONNX_DIR/lib
export DYLD_LIBRARY_PATH=$ONNX_DIR/lib:\$DYLD_LIBRARY_PATH
EOF
echo "  Created .env.macos file"

# Step 4: Setup Python environment for model export
echo
echo "[4/5] Setting up Python environment..."
if [ ! -d ".venv" ]; then
    echo "  Creating virtual environment..."
    python3 -m venv .venv
    echo "  Installing dependencies..."
    source .venv/bin/activate
    pip install -q torch gliner onnxscript
    echo "  Python dependencies installed"
else
    echo "  Virtual environment already exists"
fi

# Step 5: Build Rust project
echo
echo "[5/5] Building Rust project..."
export ORT_STRATEGY=system
export ORT_LIB_LOCATION="$ONNX_DIR/lib"

if cargo build --release 2>&1 | tail -5; then
    echo
    echo "✅ Build successful!"
    echo
    echo "Binaries location:"
    ls -lh target/release/gliner-{server,client} 2>/dev/null || true
else
    echo
    echo "❌ Build failed. Check error messages above."
    exit 1
fi

echo
echo "=== Setup Complete ==="
echo
echo "IMPORTANT: Before running the server, you must set environment variables:"
echo
echo "Option 1 - Source the env file (recommended):"
echo "  source .env.macos"
echo
echo "Option 2 - Set manually:"
echo "  export DYLD_LIBRARY_PATH=$ONNX_DIR/lib:\$DYLD_LIBRARY_PATH"
echo
echo "Then run the server:"
echo "  ./target/release/gliner-server --artifacts models"
echo
echo "Or install to ~/.cargo/bin:"
echo "  cargo install --path ."
echo "  gliner-server --artifacts models"
echo

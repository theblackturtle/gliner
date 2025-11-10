#!/bin/bash
#
# GLiNER Environment Setup Script
# Fixes ONNX Runtime CDN issues and sets up build environment
#

set -e

echo "=== GLiNER Environment Setup ==="
echo

# Step 1: Download ONNX Runtime
echo "[1/5] Downloading ONNX Runtime..."
ONNX_VERSION="1.20.1"
ONNX_DIR="$HOME/.local/onnxruntime"

if [ ! -d "$ONNX_DIR" ]; then
    echo "  Downloading ONNX Runtime v${ONNX_VERSION}..."
    cd /tmp
    wget -q --show-progress "https://github.com/microsoft/onnxruntime/releases/download/v${ONNX_VERSION}/onnxruntime-linux-x64-${ONNX_VERSION}.tgz"
    tar -xzf "onnxruntime-linux-x64-${ONNX_VERSION}.tgz"

    echo "  Installing to $ONNX_DIR..."
    mkdir -p "$ONNX_DIR"
    cp -r "onnxruntime-linux-x64-${ONNX_VERSION}"/* "$ONNX_DIR/"

    echo "  Cleaning up..."
    rm -f "onnxruntime-linux-x64-${ONNX_VERSION}.tgz"
    rm -rf "onnxruntime-linux-x64-${ONNX_VERSION}"
    cd - > /dev/null
else
    echo "  ONNX Runtime already installed at $ONNX_DIR"
fi

# Step 2: Set environment variables
echo
echo "[2/5] Setting environment variables..."
export ORT_STRATEGY=system
export ORT_LIB_LOCATION="$ONNX_DIR/lib"
export LD_LIBRARY_PATH="$ONNX_DIR/lib:$LD_LIBRARY_PATH"

echo "  ORT_STRATEGY=system"
echo "  ORT_LIB_LOCATION=$ONNX_DIR/lib"
echo "  LD_LIBRARY_PATH=$ONNX_DIR/lib:\$LD_LIBRARY_PATH"

# Step 3: Create environment file
echo
echo "[3/5] Creating .env file..."
cat > .env << EOF
# GLiNER Environment Variables
ORT_STRATEGY=system
ORT_LIB_LOCATION=$ONNX_DIR/lib
LD_LIBRARY_PATH=$ONNX_DIR/lib:\$LD_LIBRARY_PATH
EOF
echo "  Created .env file"

# Step 4: Setup Python environment for model export
echo
echo "[4/5] Setting up Python environment..."
if [ ! -d ".venv" ]; then
    echo "  Creating virtual environment..."
    uv venv .venv
    echo "  Installing dependencies..."
    source .venv/bin/activate
    uv pip install -q torch gliner onnxscript
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
echo "To use the environment variables in current shell:"
echo "  source .env"
echo
echo "To export GLiNER model to ONNX (required before running server):"
echo "  source .venv/bin/activate"
echo "  python scripts/export_gliner_onnx.py --model urchade/gliner_small-v2.1 --output models"
echo
echo "To run the server:"
echo "  source .env"
echo "  ./target/release/gliner-server --artifacts models"
echo

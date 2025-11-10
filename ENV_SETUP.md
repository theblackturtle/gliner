# GLiNER Environment Setup Guide

## ✅ Environment Fixed

The following issues have been resolved:

### 1. ONNX Runtime CDN Error (503)

**Problem:**
```
Failed to GET `https://cdn.pyke.io/0/pyke:ort-rs/ms@1.22.0/x86_64-unknown-linux-gnu.tgz`: http status: 503
```

**Root Cause:**
- The `ort-sys` crate tries to download ONNX Runtime binaries from `cdn.pyke.io`
- This CDN is currently unavailable (HTTP 503 error)
- This blocks Rust project compilation

**Solution:**
- Download ONNX Runtime manually from Microsoft's GitHub releases
- Configure `ort-sys` to use local binaries via environment variables
- ONNX Runtime v1.20.1 installed to `~/.local/onnxruntime/`

### 2. Missing Model Files

**Problem:**
- Server requires ONNX model files to run
- `models/` directory is empty

**Current Status:**
- Python environment setup complete
- GLiNER export script needs updating for latest GLiNER API

---

## 🚀 Quick Start

### Automated Setup

Run the setup script to configure everything:

```bash
chmod +x setup_env.sh
./setup_env.sh
```

This script will:
1. ✅ Download and install ONNX Runtime
2. ✅ Set up environment variables
3. ✅ Create Python virtual environment
4. ✅ Install Python dependencies
5. ✅ Build Rust binaries

### Manual Setup

If you prefer manual setup:

#### 1. Download ONNX Runtime

```bash
cd /tmp
wget https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-1.20.1.tgz
tar -xzf onnxruntime-linux-x64-1.20.1.tgz
mkdir -p ~/.local/onnxruntime
cp -r onnxruntime-linux-x64-1.20.1/* ~/.local/onnxruntime/
```

#### 2. Set Environment Variables

```bash
export ORT_STRATEGY=system
export ORT_LIB_LOCATION=$HOME/.local/onnxruntime/lib
export LD_LIBRARY_PATH=$HOME/.local/onnxruntime/lib:$LD_LIBRARY_PATH
```

Or source the `.env` file:

```bash
source .env
```

#### 3. Build Project

```bash
cargo build --release
```

The binaries will be at:
- `target/release/gliner-server`
- `target/release/gliner-client`

---

## ⚠️ Known Issues

### GLiNER ONNX Export

The current `scripts/export_gliner_onnx.py` has compatibility issues with the latest GLiNER library.

**Issue:**
- GLiNER API changed: `tokenizer` → `data_processor.transformer_tokenizer`
- Model forward signature requires multiple complex inputs
- PyTorch 2.9 dynamo mode causes export failures

**Attempted Fixes:**
- ✅ Updated tokenizer access path
- ✅ Disabled dynamo mode (`dynamo=False`)
- ❌ Model still requires decoder inputs not handled by current script

**Workarounds:**

1. **Use official GLiNER export method** (if available in newer versions)
2. **Downgrade PyTorch to 2.1.x**:
   ```bash
   source .venv/bin/activate
   uv pip install 'torch<2.2' --force-reinstall
   ```
3. **Use pre-exported ONNX model** from Hugging Face (if available)
4. **Manually prepare all model inputs** in export script

---

## 📝 Environment Variables Reference

| Variable | Value | Purpose |
|----------|-------|---------|
| `ORT_STRATEGY` | `system` | Tell ort-sys to use system-provided ONNX Runtime |
| `ORT_LIB_LOCATION` | `~/.local/onnxruntime/lib` | Path to ONNX Runtime libraries |
| `LD_LIBRARY_PATH` | Includes ONNX lib path | Runtime library search path |

---

## 🧪 Testing the Build

### Test 1: Verify Binaries

```bash
ls -lh target/release/gliner-{server,client}
```

Expected output:
```
-rwxr-xr-x gliner-client   (3-4 MB)
-rwxr-xr-x gliner-server   (5-6 MB)
```

### Test 2: Check Dependencies

```bash
ldd target/release/gliner-server | grep onnx
```

Should show:
```
libonnxruntime.so.1 => /home/user/.local/onnxruntime/lib/libonnxruntime.so.1
```

### Test 3: Run Server (will fail without model, but verifies binary works)

```bash
source .env
./target/release/gliner-server --help
```

Expected: Help message displayed

---

## 🔧 Troubleshooting

### Build fails with "cannot find -lonnxruntime"

**Solution:** Make sure environment variables are set:
```bash
source .env
cargo clean
cargo build --release
```

### Server crashes at startup

**Symptom:**
```
Error: Unable to load model artefacts
Caused by: Could not find .onnx model in directory
```

**Cause:** No model files in `models/` directory

**Solution:** Export model first (see Known Issues section for current status)

### Runtime error: "libonnxruntime.so.1 not found"

**Solution:** Set LD_LIBRARY_PATH:
```bash
export LD_LIBRARY_PATH=$HOME/.local/onnxruntime/lib:$LD_LIBRARY_PATH
```

Or add to shell profile (~/.bashrc or ~/.zshrc):
```bash
echo 'export LD_LIBRARY_PATH=$HOME/.local/onnxruntime/lib:$LD_LIBRARY_PATH' >> ~/.bashrc
source ~/.bashrc
```

---

## ✨ Build Status Summary

| Component | Status | Notes |
|-----------|--------|-------|
| ONNX Runtime | ✅ Fixed | Manual download from GitHub |
| Rust Dependencies | ✅ OK | All crates compiled |
| Rust Build | ✅ Success | Binaries created |
| Python Environment | ✅ OK | torch, gliner, onnxscript installed |
| ONNX Model Export | ⚠️ Blocked | GLiNER API compatibility issues |
| Server Runtime | 🔄 Pending | Waiting for model export |

---

## 📚 Additional Resources

- [ONNX Runtime Releases](https://github.com/microsoft/onnxruntime/releases)
- [ort-sys Documentation](https://docs.rs/ort-sys)
- [GLiNER GitHub](https://github.com/urchade/GLiNER)
- [GLiNER on Hugging Face](https://huggingface.co/urchade/gliner_small-v2.1)

---

## 🎯 Next Steps

1. ✅ Environment is now ready for development
2. ✅ Rust compilation works
3. ⚠️ Model export needs fixing:
   - Contact GLiNER maintainers for ONNX export guidance
   - Or use alternative GLiNER versions with ONNX support
   - Or write custom export script handling all model inputs
4. Once model is exported, server should run successfully

---

*Last updated: 2025-11-10*
*Environment: Linux x86_64, Rust 1.91.0, Python 3.11.14*

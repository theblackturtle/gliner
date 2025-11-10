# macOS ONNX Runtime Fix

## 🔴 Error You're Seeing

```
thread 'main' panicked at ... ort-2.0.0-rc.10/src/lib.rs:111:33:
An error occurred while attempting to load the ONNX Runtime binary at `libonnxruntime.dylib`:
dlopen(libonnxruntime.dylib, 0x0005): tried: 'libonnxruntime.dylib' (no such file)
```

## 🎯 Root Cause

The `ort` crate with `load-dynamic` feature tries to load ONNX Runtime library at runtime, but can't find `libonnxruntime.dylib`.

On macOS, you need to:
1. Download ONNX Runtime for macOS
2. Set `DYLD_LIBRARY_PATH` (macOS equivalent of Linux's `LD_LIBRARY_PATH`)

## ✅ Quick Fix

### Option 1: Automated Setup (Recommended)

```bash
# Run the macOS setup script
chmod +x setup_env_macos.sh
./setup_env_macos.sh

# Load environment variables
source .env.macos

# Run server
./target/release/gliner-server --artifacts ~/Repos/gliner/models/urchade_gliner_small-v2.1 --port 8888
```

### Option 2: Manual Setup

#### 1. Download ONNX Runtime for macOS

**For Apple Silicon (M1/M2/M3):**
```bash
cd /tmp
curl -LO https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-osx-arm64-1.20.1.tgz
tar -xzf onnxruntime-osx-arm64-1.20.1.tgz
mkdir -p ~/.local/onnxruntime
cp -r onnxruntime-osx-arm64-1.20.1/* ~/.local/onnxruntime/
```

**For Intel Mac:**
```bash
cd /tmp
curl -LO https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-osx-x86_64-1.20.1.tgz
tar -xzf onnxruntime-osx-x86_64-1.20.1.tgz
mkdir -p ~/.local/onnxruntime
cp -r onnxruntime-osx-x86_64-1.20.1/* ~/.local/onnxruntime/
```

#### 2. Set Environment Variables

**Temporary (current shell only):**
```bash
export DYLD_LIBRARY_PATH=$HOME/.local/onnxruntime/lib:$DYLD_LIBRARY_PATH
```

**Permanent (add to ~/.zshrc or ~/.bash_profile):**
```bash
echo 'export DYLD_LIBRARY_PATH=$HOME/.local/onnxruntime/lib:$DYLD_LIBRARY_PATH' >> ~/.zshrc
source ~/.zshrc
```

#### 3. Run Server

```bash
./target/release/gliner-server \
  --artifacts ~/Repos/gliner/models/urchade_gliner_small-v2.1 \
  --port 8888
```

## 🧪 Verify Setup

### Check if library is found:

```bash
# Should show path to libonnxruntime.dylib
otool -L target/release/gliner-server | grep onnx
```

### Check environment variable:

```bash
echo $DYLD_LIBRARY_PATH
# Should include: /Users/YOUR_USERNAME/.local/onnxruntime/lib
```

### Test server startup:

```bash
source .env.macos  # or set DYLD_LIBRARY_PATH manually
./target/release/gliner-server --help
```

Should display help message without panicking.

## 🐛 Still Having Issues?

### Error: "library not loaded"

**Solution 1:** Check if library exists:
```bash
ls -la ~/.local/onnxruntime/lib/libonnxruntime.*.dylib
```

**Solution 2:** Set absolute path:
```bash
export DYLD_LIBRARY_PATH=/Users/$(whoami)/.local/onnxruntime/lib:$DYLD_LIBRARY_PATH
```

### Error: "cannot be opened because the developer cannot be verified"

macOS Gatekeeper is blocking the library. Remove quarantine attribute:

```bash
xattr -d com.apple.quarantine ~/.local/onnxruntime/lib/libonnxruntime.*.dylib
```

### Error: "Image not found" with @rpath

The binary is looking for library at wrong path. Verify with:

```bash
otool -L target/release/gliner-server
```

If you see `@rpath/libonnxruntime.dylib`, the environment variable should fix it.

## 📋 Complete Run Command

Here's the complete command with all needed env vars:

```bash
# Set env (one-time per shell session)
export DYLD_LIBRARY_PATH=$HOME/.local/onnxruntime/lib:$DYLD_LIBRARY_PATH

# Run server
./target/release/gliner-server \
  --artifacts ~/Repos/gliner/models/urchade_gliner_small-v2.1 \
  --port 8888 \
  --log-level debug
```

## 🎯 Alternative: Build with Static Linking

If you want to avoid runtime library loading:

```toml
# In Cargo.toml, change:
# ort = { version = "2.0.0-rc.10", features = ["load-dynamic"] }

# To:
ort = { version = "2.0.0-rc.10", default-features = false }
```

Then set build-time env vars:
```bash
export ORT_STRATEGY=system
export ORT_LIB_LOCATION=$HOME/.local/onnxruntime/lib
cargo build --release
```

This will link the library at build time instead of runtime.

## 📚 Related Resources

- [ONNX Runtime Releases](https://github.com/microsoft/onnxruntime/releases)
- [ort crate documentation](https://docs.rs/ort)
- [macOS dyld documentation](https://developer.apple.com/library/archive/documentation/DeveloperTools/Conceptual/DynamicLibraries/)

---

## 🆘 Quick Debug Checklist

- [ ] Downloaded ONNX Runtime for macOS (correct architecture)
- [ ] Extracted to `~/.local/onnxruntime/`
- [ ] Set `DYLD_LIBRARY_PATH` in current shell
- [ ] Library file exists at `~/.local/onnxruntime/lib/libonnxruntime.*.dylib`
- [ ] No quarantine attribute on dylib file
- [ ] Server binary exists at `target/release/gliner-server`
- [ ] Model files exist at specified `--artifacts` path

Once all checked, the server should start without the dylib loading error!

# GLiNER PII Detection HTTP Server

A production-ready FastAPI server for detecting Personally Identifiable Information (PII) in files using GLiNER (Generalist and Lightweight Named Entity Recognition) models. Optimized for processing large files (50MB+) with automatic chunking and parallel processing.

## Features

- 🚀 **High Performance**: Automatic chunking, batch processing, and parallel execution
- 🔍 **Comprehensive PII Detection**: 13 core labels (fast) or 30+ extended labels (thorough)
- 🎯 **Large File Support**: Handles 50MB+ files with smart chunking mechanism
- 💻 **GPU Acceleration**: Auto-detects MPS (Apple Silicon), CUDA (NVIDIA), or CPU
- 🐳 **Docker Ready**: Containerized deployment with Docker Compose
- 🛠️ **REST API**: FastAPI with automatic OpenAPI documentation
- 📊 **Rich CLI Client**: Beautiful terminal UI with progress indicators

## Installation

### Native Installation (Recommended for GPU Support)

```bash
# Using uv (10-100x faster than pip)
curl -LsSf https://astral.sh/uv/install.sh | sh
uv pip install -r requirements.txt --system
uv pip install -r requirements-client.txt --system  # Optional: CLI client

# Or using pip
pip install -r requirements.txt
pip install -r requirements-client.txt  # Optional: CLI client
```

> **Note**: Use `--system` flag to install to current Python environment, or create a venv first with `uv venv`.

### Docker (CPU-only on macOS)

```bash
docker-compose up -d
```

> **⚠️ GPU Limitation**: Docker on macOS cannot access Metal/MPS GPU (runs in Linux VM). For GPU acceleration on Apple Silicon, use native installation.

## Quick Start

### Start the Server

```bash
# Native (auto-detects GPU)
./run_server.sh

# Or manually
python server.py

# Docker
docker-compose up -d
```

Server available at `http://localhost:8000`

**API Documentation**: http://localhost:8000/docs

## API Endpoints

### GET /api/v1/health

Check server health and model status.

```json
{
  "status": "healthy",
  "model_name": "urchade/gliner_small-v2.1",
  "device": "MPS",
  "gpu_enabled": true,
  "labels_count": 13
}
```

### POST /api/v1/scan/upload

Upload and scan a file for PII.

**Parameters:**
- `file` (required): File to upload
- `threshold` (optional): Confidence threshold (0.0-1.0, default: 0.3)
- `chunk_size` (optional): Characters per chunk (default: 8000)
- `batch_size` (optional): Chunks to process in parallel (default: 8)
- `extended_labels` (optional): Use extended label set (default: false)

**Example:**
```bash
curl -X POST "http://localhost:8000/api/v1/scan/upload" \
  -F "file=@large_file.log" \
  -F "chunk_size=10000" \
  -F "batch_size=16"
```

### POST /api/v1/scan/path

Scan an existing file or directory path.

```json
{
  "path": "/path/to/scan",
  "recursive": true,
  "labels": ["person", "email", "phone"],
  "threshold": 0.3,
  "max_workers": 4
}
```

## Client CLI

### Usage

```bash
# Check server health
python client.py health

# Upload and scan a file
python client.py upload /path/to/file.log \
  --chunk-size 10000 \
  --batch-size 16 \
  --output results.json

# Scan a directory
python client.py scan-path /data/logs \
  --recursive \
  --max-workers 8 \
  --labels person,email,phone,ssn
```

**Key Options:**
- `--server`: Server URL (default: http://localhost:8000)
- `--threshold`: Confidence threshold (0.0-1.0)
- `--chunk-size`: Characters per chunk
- `--batch-size`: Chunks to process in parallel
- `--extended-labels`: Use extended label set
- `--output`, `-o`: Save results to JSON file

## Configuration

### Environment Variables

```bash
# Model
GLINER_MODEL_NAME=urchade/gliner_small-v2.1
GLINER_THRESHOLD=0.3
GLINER_USE_GPU=true

# Performance
GLINER_CHUNK_SIZE=8000
GLINER_BATCH_SIZE=8
GLINER_MAX_FILE_SIZE=50

# Server
GLINER_HOST=0.0.0.0
GLINER_PORT=8000
```

### Available Models

| Model | Size | Speed | Accuracy | Best For |
|-------|------|-------|----------|----------|
| `urchade/gliner_small-v2.1` | Small | Fastest | Good | **Default - Production** |
| `urchade/gliner_medium-v2.1` | Medium | Fast | Better | Balanced use cases |
| `urchade/gliner_large-v2.1` | Large | Slower | Best | High accuracy |

### PII Labels

**Default Labels (13):**
person, email, phone, address, credit card, ssn, account number, password, username, ip address, date, location, organization

**Extended Labels (30+):**
All default labels plus: first name, last name, dob, age, gender, url, street, city, state, country, zip, bank account, routing number, cvv, money, medical, drug, medication, passport, driver license, license plate, company

## GPU Support

### Apple Silicon (M-series)

**Native Installation (Required for GPU):**
```bash
./run_server.sh  # Auto-detects MPS GPU
```

- Expected performance: 5-10x faster than CPU
- Docker **cannot** access Metal/MPS (runs in Linux VM)

### NVIDIA GPU (Linux)

**Native:**
```bash
pip install torch --index-url https://download.pytorch.org/whl/cu121
GLINER_USE_GPU=true python server.py
```

**Docker:**
```bash
# Requires nvidia-docker runtime
docker run --gpus all -p 8000:8000 \
  -e GLINER_USE_GPU=true \
  gliner-server
```

**Summary:**

| Platform | Native | Docker |
|----------|--------|--------|
| **Mac (Apple Silicon)** | ✅ MPS GPU | ❌ CPU only |
| **Linux (NVIDIA)** | ✅ CUDA | ✅ CUDA |
| **Other platforms** | ❌ CPU only | ❌ CPU only |

## Performance Tuning

**Large Files (50MB+):**
```bash
GLINER_CHUNK_SIZE=16000
GLINER_BATCH_SIZE=16
GLINER_MAX_FILE_SIZE=100
```

**Many Small Files:**
```bash
python client.py scan-path /data --max-workers 8
```

**Memory Optimization:**
- Use smaller model: `urchade/gliner_small-v2.1`
- Reduce batch size: `GLINER_BATCH_SIZE=4`
- Reduce chunk size: `GLINER_CHUNK_SIZE=4000`

## Troubleshooting

### Model fails to load
```bash
rm -rf ~/.cache/huggingface
GLINER_MODEL_NAME=urchade/gliner_small-v2.1 python server.py
```

### Out of memory
```bash
GLINER_BATCH_SIZE=4 GLINER_CHUNK_SIZE=4000 python server.py
```

### GPU not detected (Mac)
```bash
python -c "import torch; print(f'MPS available: {torch.backends.mps.is_available()}')"
./run_server.sh
```

### Docker connection issues
```bash
docker ps
docker logs gliner-pii-server
curl http://localhost:8000/api/v1/health
```

## Development

```bash
# Development mode with auto-reload
uvicorn server:app --reload

# Test server
curl http://localhost:8000/api/v1/health
python client.py health
```

## File Structure

```
gliner/
├── server.py              # FastAPI HTTP server
├── scanner.py             # PII scanner module
├── labels.py              # PII label definitions
├── config.py              # Configuration management
├── client.py              # Python CLI client
├── run_server.sh          # Native run script
├── Dockerfile             # Docker image
├── docker-compose.yml     # Docker Compose config
├── requirements.txt       # Server dependencies
└── requirements-client.txt # Client dependencies
```

## License

This project uses the GLiNER model which is licensed under Apache 2.0.

## References

- [GLiNER on Hugging Face](https://huggingface.co/urchade/gliner_small-v2.1)
- [GLiNER Paper](https://arxiv.org/abs/2311.08526)
- [FastAPI Documentation](https://fastapi.tiangolo.com/)

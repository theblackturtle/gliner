# GLiNER PII Detection HTTP Server

FastAPI service and CLI for detecting Personally Identifiable Information (PII) with GLiNER models.

## Highlights

- 🚀 Handles large files with chunking + parallelism
- 🔍 Supports default or extended PII label sets
- 🧠 Auto-selects CPU, CUDA, or Apple MPS for inference
- 🌐 Installs `gliner-server` and `gliner-client` commands globally

## Install

```bash
# Install uv (if needed)
curl -LsSf https://astral.sh/uv/install.sh | sh

# Install both server and client globally
uv pip install --system .

# Alternatively, register the CLI via uv tool (shows up in `uv tool list`)
uv tool install --from . gliner-server
```

## Run the Server

```bash
gliner-server
```

The API is available at `http://localhost:8000` with docs at `/docs`.

## Client CLI

```bash
# Health check
gliner-client health

# Scan local or remote paths
gliner-client scan-path /data/logs \
  --recursive \
  --labels person,email,phone
```

Key options:

- `--server`: Target server URL (default `http://localhost:8000`)
- `--threshold`: Confidence threshold (0.0–1.0)
- `--chunk-size` / `--batch-size`: Performance tuning
- `--extended-labels`: Enable extended label set

## Configuration

Environment variables let you override defaults:

```bash
GLINER_MODEL_NAME=urchade/gliner_small-v2.1
GLINER_THRESHOLD=0.3
GLINER_CHUNK_SIZE=8000
GLINER_BATCH_SIZE=8
GLINER_MAX_FILE_SIZE=50
GLINER_HOST=0.0.0.0
GLINER_PORT=8000
```

## GPU Notes

- **Apple Silicon**: `gliner-server` automatically enables MPS.
- **Linux (CUDA)**: install the matching PyTorch wheel, then run `GLINER_USE_GPU=true gliner-server`.

## Development

```bash
uv pip install --editable .
uvicorn server:app --reload
```

## Project Layout

```
gliner/
├── server.py
├── scanner.py
├── labels.py
├── config.py
├── client.py
├── requirements*.txt
└── pyproject.toml
```

## License

Apache 2.0 (via upstream GLiNER model).

## References

- [GLiNER on Hugging Face](https://huggingface.co/urchade/gliner_small-v2.1)
- [GLiNER Paper](https://arxiv.org/abs/2311.08526)
- [FastAPI Documentation](https://fastapi.tiangolo.com/)

# Multi-stage Dockerfile for GLiNER PII Detection Server
# Optimized for production deployment (CPU mode)

# Stage 1: Builder
FROM python:3.11-slim as builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc \
    g++ \
    && rm -rf /var/lib/apt/lists/*

# Install uv for faster dependency installation
RUN pip install --no-cache-dir uv

# Copy requirements first for better caching
COPY requirements.txt .

# Install Python dependencies using uv (10-100x faster than pip)
# Install to /opt/venv to avoid permission issues
RUN uv venv /opt/venv && \
    /opt/venv/bin/pip install -r requirements.txt

# Stage 2: Runtime
FROM python:3.11-slim

WORKDIR /app

# Create non-root user for security
RUN useradd -m -u 1000 gliner && \
    mkdir -p /app /data && \
    chown -R gliner:gliner /app /data

# Copy virtual environment from builder
COPY --from=builder /opt/venv /opt/venv

# Copy application files
COPY --chown=gliner:gliner labels.py .
COPY --chown=gliner:gliner config.py .
COPY --chown=gliner:gliner scanner.py .
COPY --chown=gliner:gliner server.py .

# Set environment variables
ENV PATH=/opt/venv/bin:$PATH \
    PYTHONUNBUFFERED=1 \
    GLINER_MODEL_NAME=urchade/gliner_small-v2.1 \
    GLINER_USE_GPU=false \
    GLINER_HOST=0.0.0.0 \
    GLINER_PORT=8000 \
    VIRTUAL_ENV=/opt/venv

# Switch to non-root user
USER gliner

# Expose port
EXPOSE 8000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD /opt/venv/bin/python -c "import requests; requests.get('http://localhost:8000/api/v1/health')" || exit 1

# Run server
CMD ["/opt/venv/bin/python", "server.py"]


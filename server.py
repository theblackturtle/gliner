"""
FastAPI HTTP Server for GLiNER PII Detection

Provides REST API endpoints for PII detection in files and directories.
Model is loaded once at startup and shared across all requests.
"""

import tempfile
import shutil
import signal
import sys
import argparse
from pathlib import Path
from contextlib import asynccontextmanager
from typing import List, Optional, Dict, Any

from fastapi import FastAPI, UploadFile, File, Form, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field
import uvicorn

from scanner import PIIScanner
from labels import PII_LABELS, EXTENDED_PII_LABELS
from config import settings

# Global scanner instance
scanner: Optional[PIIScanner] = None


def signal_handler(sig, frame):
    """Handle interrupt signals for graceful shutdown."""
    print("\n\nInterrupt received, shutting down gracefully...")
    print("Please wait for active requests to complete...")
    sys.exit(0)


# Register signal handlers
signal.signal(signal.SIGINT, signal_handler)
signal.signal(signal.SIGTERM, signal_handler)


# Pydantic Models
class PIIEntity(BaseModel):
    """Detected PII entity."""
    text: str
    label: str
    score: float
    start: int
    end: int


class ScanResult(BaseModel):
    """Result of scanning a single file."""
    file: str
    status: str
    entities: List[PIIEntity] = []
    pii_count: int = 0
    file_size_mb: Optional[float] = None
    chunks_processed: Optional[int] = None
    error: Optional[str] = None


class ScanSummary(BaseModel):
    """Summary statistics for scan operation."""
    total_files: int
    files_with_pii: int
    total_pii: int
    errors: int


class ScanResponse(BaseModel):
    """Response for scan operations."""
    results: List[ScanResult]
    summary: ScanSummary


class ScanPathRequest(BaseModel):
    """Request body for scanning a path."""
    path: str = Field(..., description="File or directory path to scan")
    recursive: bool = Field(default=True, description="Recursively scan subdirectories")
    labels: Optional[List[str]] = Field(default=None, description="Custom PII labels (uses default if not provided)")
    threshold: float = Field(default=0.3, ge=0.0, le=1.0, description="Confidence threshold")
    chunk_size: int = Field(default=8000, gt=0, description="Characters per chunk")
    batch_size: int = Field(default=8, gt=0, description="Chunks to process in parallel")
    max_workers: Optional[int] = Field(default=None, description="Parallel workers for directory scanning")
    max_file_size: float = Field(default=50.0, gt=0, description="Maximum file size in MB")
    extended_labels: bool = Field(default=False, description="Use extended label set")
    filter_false_positives: bool = Field(default=True, description="Enable false positive filtering")


class HealthResponse(BaseModel):
    """Health check response."""
    status: str
    model_name: str
    device: str
    gpu_enabled: bool
    labels_count: int


class InfoResponse(BaseModel):
    """Server information response."""
    model_name: str
    device: str
    gpu_enabled: bool
    default_labels: List[str]
    extended_labels: List[str]
    default_config: Dict[str, Any]


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Lifespan context manager to load model at startup."""
    global scanner
    
    print(f"Loading GLiNER model: {settings.model_name}")
    scanner = PIIScanner(
        model_name=settings.model_name,
        threshold=settings.threshold,
        max_chunk_size=settings.chunk_size,
        batch_size=settings.batch_size,
        use_gpu=settings.use_gpu,
        filter_false_positives=settings.filter_false_positives
    )
    
    model_info = scanner.load_model()
    print(f"Model loaded successfully on {model_info['device']}")
    
    yield
    
    # Cleanup
    print("\nShutting down server...")
    print("Server stopped successfully")


# Create FastAPI app
app = FastAPI(
    title="GLiNER PII Detection API",
    description="REST API for detecting Personally Identifiable Information in text files using GLiNER",
    version="1.0.0",
    lifespan=lifespan
)

# Add CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=settings.cors_origins,
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


@app.get("/", tags=["Root"])
async def root():
    """Root endpoint."""
    return {
        "message": "GLiNER PII Detection API",
        "version": "1.0.0",
        "endpoints": {
            "health": "/api/v1/health",
            "info": "/api/v1/info",
            "scan_upload": "/api/v1/scan/upload",
            "scan_path": "/api/v1/scan/path",
            "docs": "/docs"
        }
    }


@app.get("/api/v1/health", response_model=HealthResponse, tags=["Health"])
async def health():
    """Check server health and model status."""
    if scanner is None or scanner.model is None:
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail="Model not loaded"
        )
    
    return HealthResponse(
        status="healthy",
        model_name=scanner.model_name,
        device=scanner.device.upper(),
        gpu_enabled=scanner.use_gpu,
        labels_count=len(PII_LABELS)
    )


@app.get("/api/v1/info", response_model=InfoResponse, tags=["Info"])
async def info():
    """Get server information and configuration."""
    if scanner is None:
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail="Model not loaded"
        )
    
    return InfoResponse(
        model_name=scanner.model_name,
        device=scanner.device.upper(),
        gpu_enabled=scanner.use_gpu,
        default_labels=PII_LABELS,
        extended_labels=EXTENDED_PII_LABELS,
        default_config={
            "threshold": settings.threshold,
            "chunk_size": settings.chunk_size,
            "batch_size": settings.batch_size,
            "max_file_size": settings.max_file_size
        }
    )


@app.post("/api/v1/scan/upload", response_model=ScanResponse, tags=["Scan"])
async def scan_upload(
    file: UploadFile = File(...),
    labels: Optional[str] = Form(default=None, description="Comma-separated PII labels"),
    threshold: float = Form(default=0.3, ge=0.0, le=1.0),
    chunk_size: int = Form(default=8000, gt=0),
    batch_size: int = Form(default=8, gt=0),
    max_file_size: float = Form(default=50.0, gt=0),
    extended_labels: bool = Form(default=False),
    filter_false_positives: bool = Form(default=True, description="Enable false positive filtering")
):
    """
    Upload and scan a file for PII.
    
    Supports large files (50MB+) with automatic chunking and batch processing.
    """
    if scanner is None:
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail="Model not loaded"
        )
    
    # Determine labels to use
    if labels:
        label_list = [label.strip() for label in labels.split(",")]
    elif extended_labels:
        label_list = EXTENDED_PII_LABELS
    else:
        label_list = PII_LABELS
    
    # Create temporary file
    temp_dir = tempfile.mkdtemp()
    temp_path = Path(temp_dir) / file.filename
    
    try:
        # Save uploaded file
        print(f"\nUpload started: {file.filename}")
        with open(temp_path, "wb") as f:
            shutil.copyfileobj(file.file, f)
        
        file_size_mb = temp_path.stat().st_size / (1024 * 1024)
        print(f"Upload complete: {file.filename} ({file_size_mb:.1f}MB)")
        print(f"Scanning with chunk_size={chunk_size}, batch_size={batch_size}, threshold={threshold}")
        
        # Update scanner settings for this request
        original_threshold = scanner.threshold
        original_chunk_size = scanner.max_chunk_size
        original_batch_size = scanner.batch_size
        original_filter_fp = scanner.filter_false_positives
        
        scanner.threshold = threshold
        scanner.max_chunk_size = chunk_size
        scanner.batch_size = batch_size
        scanner.filter_false_positives = filter_false_positives
        
        # Scan file
        result = scanner.scan_file(temp_path, label_list, max_file_size)
        
        # Restore original settings
        scanner.threshold = original_threshold
        scanner.max_chunk_size = original_chunk_size
        scanner.batch_size = original_batch_size
        scanner.filter_false_positives = original_filter_fp
        
        # Log results
        if result['status'] == 'success':
            chunks = result.get('chunks_processed', 1)
            pii_count = result.get('pii_count', 0)
            print(f"Scan complete: {chunks} chunks processed, {pii_count} PII entities found")
        else:
            print(f"Scan {result['status']}: {result.get('error', 'Unknown error')}")
        
        # Calculate summary
        summary = ScanSummary(
            total_files=1,
            files_with_pii=1 if result['status'] == 'success' and result['pii_count'] > 0 else 0,
            total_pii=result.get('pii_count', 0),
            errors=1 if result['status'] == 'error' else 0
        )
        
        return ScanResponse(
            results=[ScanResult(**result)],
            summary=summary
        )
        
    finally:
        # Cleanup temp file
        shutil.rmtree(temp_dir, ignore_errors=True)


@app.post("/api/v1/scan/path", response_model=ScanResponse, tags=["Scan"])
async def scan_path(request: ScanPathRequest):
    """
    Scan a file or directory path for PII.
    
    Server must have filesystem access to the specified path.
    Supports recursive directory scanning with parallel processing.
    """
    if scanner is None:
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail="Model not loaded"
        )
    
    path = Path(request.path)
    
    if not path.exists():
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Path not found: {request.path}"
        )
    
    # Determine labels to use
    if request.labels:
        label_list = request.labels
    elif request.extended_labels:
        label_list = EXTENDED_PII_LABELS
    else:
        label_list = PII_LABELS
    
    # Update scanner settings for this request
    original_threshold = scanner.threshold
    original_chunk_size = scanner.max_chunk_size
    original_batch_size = scanner.batch_size
    original_filter_fp = scanner.filter_false_positives
    
    scanner.threshold = request.threshold
    scanner.max_chunk_size = request.chunk_size
    scanner.batch_size = request.batch_size
    scanner.filter_false_positives = request.filter_false_positives
    
    try:
        # Log scan start
        scan_type = "file" if path.is_file() else "directory"
        print(f"\nScanning {scan_type}: {request.path}")
        print(f"   Settings: chunk_size={request.chunk_size}, batch_size={request.batch_size}, threshold={request.threshold}, filter_fp={request.filter_false_positives}")
        
        # Scan file or directory
        if path.is_file():
            results = [scanner.scan_file(path, label_list, request.max_file_size)]
        else:
            results = scanner.scan_directory(
                path,
                label_list,
                recursive=request.recursive,
                max_workers=request.max_workers,
                max_file_size_mb=request.max_file_size
            )
        
        # Calculate summary
        total_files = len(results)
        files_with_pii = sum(1 for r in results if r['status'] == 'success' and r.get('pii_count', 0) > 0)
        total_pii = sum(r.get('pii_count', 0) for r in results if r['status'] == 'success')
        errors = sum(1 for r in results if r['status'] == 'error')
        
        # Log summary
        print(f"Scan complete: {total_files} files, {files_with_pii} with PII, {total_pii} entities found")
        if errors > 0:
            print(f"Errors: {errors} files failed to scan")
        
        summary = ScanSummary(
            total_files=total_files,
            files_with_pii=files_with_pii,
            total_pii=total_pii,
            errors=errors
        )
        
        # Convert to response models
        scan_results = [ScanResult(**result) for result in results]
        
        return ScanResponse(results=scan_results, summary=summary)
        
    finally:
        # Restore original settings
        scanner.threshold = original_threshold
        scanner.max_chunk_size = original_chunk_size
        scanner.batch_size = original_batch_size
        scanner.filter_false_positives = original_filter_fp


def parse_args():
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(
        description="GLiNER PII Detection HTTP Server - FastAPI server for detecting PII in files",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  gliner-server
  gliner-server -m urchade/gliner_medium-v2.1 -t 0.5
  gliner-server -m urchade/gliner_large-v2.1 -t 0.6 --no-filter
  gliner-server -p 8080 --reload

Available Models:
  urchade/gliner_small-v2.1   - Fastest, good accuracy (default)
  urchade/gliner_medium-v2.1  - Balanced, very good accuracy
  urchade/gliner_large-v2.1   - Slower, excellent accuracy

API Docs: http://localhost:8000/docs
        """
    )
    
    # Model Configuration
    parser.add_argument(
        '-m', '--model',
        type=str,
        default=settings.model_name,
        help=f'GLiNER model name (default: {settings.model_name})'
    )
    parser.add_argument(
        '-t', '--threshold',
        type=float,
        default=settings.threshold,
        help=f'Confidence threshold 0.0-1.0 (default: {settings.threshold})'
    )
    parser.add_argument(
        '--no-gpu',
        action='store_true',
        help='Disable GPU acceleration'
    )
    parser.add_argument(
        '--no-filter',
        action='store_true',
        help='Disable false positive filtering'
    )
    
    # Server Configuration
    parser.add_argument(
        '--host',
        type=str,
        default=settings.host,
        help=f'Server host (default: {settings.host})'
    )
    parser.add_argument(
        '-p', '--port',
        type=int,
        default=settings.port,
        help=f'Server port (default: {settings.port})'
    )
    parser.add_argument(
        '--reload',
        action='store_true',
        help='Enable auto-reload on code changes (development only)'
    )
    parser.add_argument(
        '--log-level',
        type=str,
        choices=['debug', 'info', 'warning', 'error'],
        default=settings.log_level,
        help=f'Logging level (default: {settings.log_level})'
    )
    
    return parser.parse_args()


def main():
    """Start the GLiNER PII detection server."""
    args = parse_args()
    
    # Print banner
    print("=" * 80)
    print("GLiNER PII Detection Server")
    print("=" * 80)
    print(f"Model: {args.model}")
    print(f"Threshold: {args.threshold}")
    print(f"GPU: {'disabled' if args.no_gpu else 'enabled (auto-detect)'}")
    print(f"False Positive Filter: {'disabled' if args.no_filter else 'enabled'}")
    print(f"Server: http://{args.host}:{args.port}")
    print(f"API Docs: http://{args.host if args.host != '0.0.0.0' else 'localhost'}:{args.port}/docs")
    print("=" * 80)
    print()
    
    # Override settings with command-line arguments
    settings.model_name = args.model
    settings.threshold = args.threshold
    settings.use_gpu = not args.no_gpu
    settings.filter_false_positives = not args.no_filter
    settings.host = args.host
    settings.port = args.port
    settings.reload = args.reload
    settings.log_level = args.log_level
    
    # Start server
    config = uvicorn.Config(
        "server:app",
        host=settings.host,
        port=settings.port,
        reload=settings.reload,
        log_level=settings.log_level,
        timeout_graceful_shutdown=3,  # Fast shutdown on Ctrl+C
        timeout_keep_alive=5,
        limit_concurrency=100
    )
    server = uvicorn.Server(config)
    server.run()


if __name__ == "__main__":
    main()


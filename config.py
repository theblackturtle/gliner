"""
Configuration Management for GLiNER PII Detection Server

Environment variables with defaults and validation using Pydantic Settings.
"""

from pydantic_settings import BaseSettings
from typing import Optional


class Settings(BaseSettings):
    """Application settings with environment variable support."""
    
    # Model Configuration
    model_name: str = "urchade/gliner_small-v2.1"
    threshold: float = 0.3
    use_gpu: bool = True  # Will auto-detect MPS for Apple Silicon
    filter_false_positives: bool = True  # Enable false positive filtering
    
    # Performance Configuration
    chunk_size: int = 8000
    batch_size: int = 8
    max_workers: Optional[int] = None  # Auto-detect based on CPU count
    max_file_size: float = 50.0  # MB
    
    # Server Configuration
    host: str = "0.0.0.0"
    port: int = 8000
    reload: bool = False
    
    # CORS Configuration
    cors_origins: list[str] = ["*"]
    
    # Logging
    log_level: str = "info"
    
    class Config:
        env_prefix = "GLINER_"
        case_sensitive = False


# Global settings instance
settings = Settings()


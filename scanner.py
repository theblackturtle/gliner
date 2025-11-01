"""
Standalone PII Scanner Module using GLiNER

This module provides a thread-safe PII scanner with support for:
- Large file processing via chunking
- Batch processing for performance
- GPU acceleration (MPS for Apple Silicon, CUDA for NVIDIA)
- Multiple file encodings
- False positive filtering
"""

import os
import warnings
import time
import re
from pathlib import Path
from typing import List, Dict, Any, Optional
from concurrent.futures import ThreadPoolExecutor, as_completed
import multiprocessing

from gliner import GLiNER
import torch

from labels import TEXT_EXTENSIONS

# Suppress GLiNER truncation warnings
warnings.filterwarnings('ignore', message='Sentence of length .* has been truncated to .*')
warnings.filterwarnings('ignore', message='Asking to truncate to max_length but no maximum length is provided.*')
warnings.filterwarnings('ignore', message='.*The sentencepiece tokenizer.*byte fallback.*')
warnings.filterwarnings('ignore', category=UserWarning, module='transformers')


# False positive patterns and field names to filter out
FALSE_POSITIVE_FILTERS = {
    'password': {
        'exact_match': {'password', 'pwd', 'passwd', 'pass'},
        'patterns': [
            r'^password$',
            r'^pwd$',
            r'password\d*$',
        ]
    },
    'email': {
        'exact_match': {'email', 'emailaddress', 'useremail', 'e-mail'},
        'patterns': [
            r'^email$',
            r'^.*email.*$',  # Any field name containing "email"
        ],
        # Valid email pattern - keep only if it matches
        'valid_pattern': r'^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$'
    },
    'phone': {
        'exact_match': {'phone', 'phonenumber', 'mobile', 'tel', 'telephone'},
        'patterns': [
            r'^iphone\d+$',  # iPhone model numbers
            r'^phone$',
            r'.*phone.*',  # Field names containing "phone"
        ],
        # Valid phone pattern - at least 10 digits
        'valid_pattern': r'.*\d.*\d.*\d.*\d.*\d.*\d.*\d.*\d.*\d.*\d.*'
    },
    'address': {
        'exact_match': {'address', 'addr', 'location', 'phonenumber', 'emailaddress', 'emailaddressvalue'},
        'patterns': [
            r'^address$',
            r'^.*address.*$',
        ]
    },
    'credit card': {
        'exact_match': {'card', 'creditcard', 'visa', 'mastercard', 'amex', 'discover'},
        'patterns': [
            r'^.*card.*$',  # Field names containing "card"
            r'^visa$',
            r'^mastercard$',
        ],
        # Valid credit card - 13-19 digits
        'valid_pattern': r'^\d{13,19}$'
    },
    'ssn': {
        'exact_match': {'ssn', 'socialsecurity', 'social'},
        'patterns': [
            r'^[a-z]{1,5}$',  # Short random strings like "xxs"
            r'^[A-Z][a-z]+$',  # CamelCase words like "Transaction", "SdkMeter"
        ],
        # Valid SSN pattern
        'valid_pattern': r'^\d{3}-?\d{2}-?\d{4}$'
    },
    'account number': {
        'patterns': [
            r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',  # UUID - might be valid but low confidence
        ],
        'min_confidence': 0.6  # Require higher confidence for account numbers
    },
    'ip address': {
        'exact_match': {'prod', 'staging', 'dev'},
        'patterns': [
            r'^[a-z]{2,3}-[a-z]+-\d+$',  # AWS regions like "us-west-2"
            r'^prod-.*$',
            r'^staging-.*$',
            r'^dev-.*$',
        ],
        # Valid IP pattern
        'valid_pattern': r'^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$'
    },
    'username': {
        'exact_match': {'username', 'user', 'userid', 'user_id'},
        'patterns': [
            r'^.*uuid$',  # Field names ending in "uuid"
            r'^.*_uuid$',
            r'^.*Uuid$',
        ]
    },
    'date': {
        'min_confidence': 0.7,  # Require higher confidence for dates
    }
}


class PIIScanner:
    """Thread-safe PII scanner using GLiNER model."""
    
    def __init__(
        self,
        model_name: str = "urchade/gliner_small-v2.1",
        threshold: float = 0.3,
        max_chunk_size: int = 8000,
        batch_size: int = 8,
        use_gpu: bool = True,
        filter_false_positives: bool = True
    ):
        """
        Initialize the PII Scanner with GLiNER model.
        
        Args:
            model_name: HuggingFace model name
            threshold: Confidence threshold for entity detection (0.0-1.0)
            max_chunk_size: Maximum characters per chunk for large files
            batch_size: Number of chunks to process in parallel
            use_gpu: Enable GPU acceleration (auto-detects MPS/CUDA)
            filter_false_positives: Enable false positive filtering
        """
        self.threshold = threshold
        self.model = None
        self.model_name = model_name
        self.max_chunk_size = max_chunk_size
        self.batch_size = batch_size
        self.filter_false_positives = filter_false_positives
        
        # Auto-detect GPU: MPS (Apple Silicon) > CUDA > CPU
        if use_gpu:
            if torch.backends.mps.is_available():
                self.device = "mps"
                self.use_gpu = True
            elif torch.cuda.is_available():
                self.device = "cuda"
                self.use_gpu = True
            else:
                self.device = "cpu"
                self.use_gpu = False
        else:
            self.device = "cpu"
            self.use_gpu = False
    
    def _is_false_positive(self, entity: Dict[str, Any]) -> bool:
        """
        Check if an entity is likely a false positive.
        
        Args:
            entity: Entity dict with 'text', 'label', and 'score'
            
        Returns:
            True if the entity should be filtered out
        """
        text = entity['text']
        label = entity['label'].lower()
        confidence = entity['score']
        
        # Get filter rules for this label
        filters = FALSE_POSITIVE_FILTERS.get(label, {})
        if not filters:
            return False
        
        # Check minimum confidence threshold
        min_conf = filters.get('min_confidence')
        if min_conf and confidence < min_conf:
            return True
        
        # Normalize text for comparison
        text_lower = text.lower().strip()
        
        # Check exact matches
        exact_matches = filters.get('exact_match', set())
        if text_lower in exact_matches:
            return True
        
        # Check regex patterns (for false positives)
        patterns = filters.get('patterns', [])
        for pattern in patterns:
            if re.match(pattern, text_lower, re.IGNORECASE):
                return True
        
        # Check if valid pattern exists and text doesn't match
        valid_pattern = filters.get('valid_pattern')
        if valid_pattern and not re.match(valid_pattern, text):
            return True
        
        return False
    
    def load_model(self) -> Dict[str, Any]:
        """
        Load the GLiNER model.
        
        Returns:
            Dict with status, device, and model info
        """
        try:
            self.model = GLiNER.from_pretrained(self.model_name)
            
            if self.use_gpu:
                self.model = self.model.to(self.device)
            
            return {
                "status": "success",
                "model_name": self.model_name,
                "device": self.device.upper(),
                "gpu_enabled": self.use_gpu
            }
        except Exception as e:
            raise RuntimeError(f"Failed to load model {self.model_name}: {str(e)}")
    
    def _chunk_text(self, text: str) -> List[tuple[str, int]]:
        """
        Split text into chunks that fit within GLiNER's token limit.
        Smart boundary detection to avoid splitting entities.
        
        Args:
            text: Text to chunk
            
        Returns:
            List of (chunk_text, start_offset) tuples
        """
        if len(text) <= self.max_chunk_size:
            return [(text, 0)]
        
        chunks = []
        start = 0
        
        while start < len(text):
            end = start + self.max_chunk_size
            
            # If not at the end, try to break at a newline or space
            if end < len(text):
                # Look back up to 500 chars for a good break point
                break_point = text.rfind('\n', start, end)
                if break_point == -1:
                    break_point = text.rfind(' ', start, end)
                if break_point != -1 and break_point > start:
                    end = break_point + 1
            
            chunks.append((text[start:end], start))
            start = end
        
        return chunks
    
    def scan_text(self, text: str, labels: List[str]) -> List[Dict[str, Any]]:
        """
        Scan text for PII entities with chunking and batch processing.
        
        Args:
            text: Text to scan
            labels: List of PII labels to detect
            
        Returns:
            List of detected entities with positions
        """
        if not self.model:
            raise RuntimeError("Model not loaded. Call load_model() first.")
        
        # Split text into manageable chunks
        chunks = self._chunk_text(text)
        
        if len(chunks) > 1:
            print(f"   Text split into {len(chunks)} chunks (chunk_size={self.max_chunk_size})")
        
        all_entities = []
        seen_entities = set()  # Deduplicate entities at chunk boundaries
        
        # Process chunks in batches for better performance
        total_batches = (len(chunks) + self.batch_size - 1) // self.batch_size
        
        for batch_idx, i in enumerate(range(0, len(chunks), self.batch_size), 1):
            batch = chunks[i:i + self.batch_size]
            batch_texts = [chunk_text for chunk_text, _ in batch]
            
            if len(chunks) > self.batch_size:
                print(f"   Processing batch {batch_idx}/{total_batches} ({len(batch)} chunks)...")
            
            # Batch prediction
            if len(batch_texts) == 1:
                batch_results = [self.model.predict_entities(
                    batch_texts[0], labels, threshold=self.threshold
                )]
            else:
                batch_results = []
                for text in batch_texts:
                    batch_results.append(self.model.predict_entities(
                        text, labels, threshold=self.threshold
                    ))
            
            # Process results and adjust positions
            for (chunk_text, offset), chunk_entities in zip(batch, batch_results):
                for entity in chunk_entities:
                    entity['start'] += offset
                    entity['end'] += offset
                    
                    # Filter false positives
                    if self.filter_false_positives and self._is_false_positive(entity):
                        continue
                    
                    # Deduplicate
                    entity_key = (entity['text'], entity['label'], entity['start'], entity['end'])
                    
                    if entity_key not in seen_entities:
                        seen_entities.add(entity_key)
                        all_entities.append(entity)
        
        return all_entities
    
    def scan_file(
        self,
        file_path: Path,
        labels: List[str],
        max_file_size_mb: float = 50
    ) -> Dict[str, Any]:
        """
        Scan a single file for PII.
        
        Args:
            file_path: Path to file
            labels: List of PII labels to detect
            max_file_size_mb: Maximum file size to process
            
        Returns:
            Scan result with entities and metadata
        """
        start_time = time.time()
        
        try:
            # Check file size
            file_size_bytes = file_path.stat().st_size
            file_size_mb = file_size_bytes / (1024 * 1024)
            
            print(f"   Scanning: {file_path.name} ({file_size_mb:.1f}MB)")
            
            if file_size_mb > max_file_size_mb:
                print(f"   Skipped: File too large ({file_size_mb:.1f}MB > {max_file_size_mb}MB)")
                return {
                    'file': str(file_path),
                    'status': 'skipped',
                    'error': f'File too large ({file_size_mb:.1f}MB > {max_file_size_mb}MB)',
                    'entities': []
                }
            
            # Try multiple encodings
            content = None
            for encoding in ['utf-8', 'latin-1', 'cp1252']:
                try:
                    with open(file_path, 'r', encoding=encoding) as f:
                        content = f.read()
                    break
                except UnicodeDecodeError:
                    continue
            
            if content is None:
                return {
                    'file': str(file_path),
                    'status': 'error',
                    'error': 'Unable to decode file',
                    'entities': []
                }
            
            # Skip empty files
            if not content.strip():
                return {
                    'file': str(file_path),
                    'status': 'skipped',
                    'error': 'Empty file',
                    'entities': []
                }
            
            # Check if chunking is needed
            chunks_needed = len(self._chunk_text(content))
            
            # Scan for PII
            entities = self.scan_text(content, labels)
            
            # Calculate elapsed time
            elapsed = time.time() - start_time
            
            # Log results
            if len(entities) > 0:
                print(f"   Found {len(entities)} PII entities ({elapsed:.1f}s)")
            else:
                print(f"   No PII found ({elapsed:.1f}s)")
            
            result = {
                'file': str(file_path),
                'status': 'success',
                'entities': entities,
                'pii_count': len(entities),
                'file_size_mb': file_size_mb
            }
            
            if chunks_needed > 1:
                result['chunks_processed'] = chunks_needed
            
            return result
            
        except Exception as e:
            print(f"   Error: {str(e)}")
            return {
                'file': str(file_path),
                'status': 'error',
                'error': str(e),
                'entities': []
            }
    
    def scan_directory(
        self,
        directory: Path,
        labels: List[str],
        recursive: bool = True,
        max_workers: Optional[int] = None,
        max_file_size_mb: float = 50
    ) -> List[Dict[str, Any]]:
        """
        Scan all text files in a directory with parallel processing.
        
        Args:
            directory: Directory path
            labels: List of PII labels to detect
            recursive: Recursively scan subdirectories
            max_workers: Number of parallel workers
            max_file_size_mb: Maximum file size to process
            
        Returns:
            List of scan results
        """
        results = []
        
        # Collect all files to scan
        files_to_scan = []
        if recursive:
            for root, _, files in os.walk(directory):
                for file in files:
                    file_path = Path(root) / file
                    if file_path.suffix.lower() in TEXT_EXTENSIONS:
                        files_to_scan.append(file_path)
        else:
            files_to_scan = [
                f for f in directory.iterdir()
                if f.is_file() and f.suffix.lower() in TEXT_EXTENSIONS
            ]
        
        if not files_to_scan:
            print(f"   No text files found in directory")
            return results
        
        print(f"   Found {len(files_to_scan)} files to scan")
        
        # Determine number of workers
        if max_workers is None:
            max_workers = min(4, multiprocessing.cpu_count())
        
        # Single-threaded for small number of files
        if len(files_to_scan) < 5 or max_workers == 1:
            for idx, file_path in enumerate(files_to_scan, 1):
                print(f"\n   [{idx}/{len(files_to_scan)}]")
                result = self.scan_file(file_path, labels, max_file_size_mb)
                results.append(result)
        else:
            # Parallel processing with ThreadPoolExecutor
            print(f"   Using {max_workers} parallel workers")
            completed = 0
            with ThreadPoolExecutor(max_workers=max_workers) as executor:
                future_to_file = {
                    executor.submit(self.scan_file, file_path, labels, max_file_size_mb): file_path
                    for file_path in files_to_scan
                }
                
                for future in as_completed(future_to_file):
                    result = future.result()
                    results.append(result)
                    completed += 1
                    if completed % 5 == 0 or completed == len(files_to_scan):
                        print(f"   Progress: {completed}/{len(files_to_scan)} files completed")
        
        return results


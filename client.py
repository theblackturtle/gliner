#!/usr/bin/env python3
"""
GLiNER PII Detection Client CLI

Command-line client for interacting with the GLiNER HTTP server.
"""

import os
import json
import sys
from pathlib import Path
from typing import Optional, List

import click
import requests
from rich.console import Console
from rich.table import Table
from rich.panel import Panel
from rich.tree import Tree
from rich.progress import Progress, SpinnerColumn, TextColumn, BarColumn, DownloadColumn, TransferSpeedColumn

console = Console()

# Default server URL from environment or localhost
DEFAULT_SERVER = os.getenv("GLINER_SERVER_URL", "http://localhost:8000")


def make_request(method: str, endpoint: str, server: str, **kwargs) -> requests.Response:
    """
    Make HTTP request with error handling and retries.
    
    Args:
        method: HTTP method (GET, POST)
        endpoint: API endpoint
        server: Server URL
        **kwargs: Additional arguments for requests
        
    Returns:
        Response object
    """
    url = f"{server}{endpoint}"
    
    try:
        response = requests.request(method, url, timeout=None, **kwargs)
        response.raise_for_status()
        return response
    except requests.exceptions.ConnectionError:
        console.print(f"[red]✗ Could not connect to server at {server}[/red]")
        console.print(f"[yellow]Make sure the server is running[/yellow]")
        sys.exit(1)
    except requests.exceptions.Timeout:
        console.print(f"[red]✗ Request timed out[/red]")
        sys.exit(1)
    except requests.exceptions.HTTPError as e:
        console.print(f"[red]✗ HTTP error: {e.response.status_code}[/red]")
        if e.response.text:
            try:
                error_detail = e.response.json()
                console.print(f"[red]{error_detail.get('detail', e.response.text)}[/red]")
            except:
                console.print(f"[red]{e.response.text}[/red]")
        sys.exit(1)
    except Exception as e:
        console.print(f"[red]✗ Error: {str(e)}[/red]")
        sys.exit(1)


def display_scan_results(data: dict, show_all: bool = False):
    """Display scan results in a formatted tree."""
    results = data.get('results', [])
    summary = data.get('summary', {})
    
    # Display summary
    total_files = summary.get('total_files', 0)
    files_with_pii = summary.get('files_with_pii', 0)
    total_pii = summary.get('total_pii', 0)
    errors = summary.get('errors', 0)
    
    pii_percent = (files_with_pii / total_files * 100) if total_files > 0 else 0
    
    summary_text = f"""
[bold]Total Files Scanned:[/bold] {total_files}
[bold]Files with PII:[/bold] {files_with_pii} [red]({pii_percent:.1f}%)[/red]
[bold]Total PII Entities Found:[/bold] {total_pii}
[bold]Errors:[/bold] {errors}
    """.strip()
    
    console.print()
    console.print(Panel(summary_text, title="[bold cyan]Scan Summary[/bold cyan]", border_style="cyan"))
    console.print()
    
    # Display detailed results
    if show_all:
        files_to_show = results
    else:
        files_to_show = [r for r in results if r['status'] == 'success' and r.get('pii_count', 0) > 0]
    
    if not files_to_show:
        if not show_all:
            console.print("[green]✓ No PII detected in any files![/green]")
        return
    
    for result in files_to_show:
        file_path = result['file']
        status = result['status']
        
        if status == 'error':
            console.print(f"[red]✗[/red] {file_path}: [red]{result.get('error', 'Unknown error')}[/red]")
            continue
        
        entities = result.get('entities', [])
        
        if not entities:
            if show_all:
                console.print(f"[green]✓[/green] {file_path}: [green]No PII detected[/green]")
            continue
        
        # Create tree for file
        file_info = f"[bold red]🔍 {file_path}[/bold red] ([red]{len(entities)} PII entities[/red])"
        
        if result.get('chunks_processed'):
            file_size_mb = result.get('file_size_mb', 0)
            chunks = result['chunks_processed']
            file_info += f" [dim]- {file_size_mb:.1f}MB processed in {chunks} chunks[/dim]"
        
        tree = Tree(file_info)
        
        # Group entities by label
        entities_by_label = {}
        for entity in entities:
            label = entity['label']
            if label not in entities_by_label:
                entities_by_label[label] = []
            entities_by_label[label].append(entity)
        
        # Add entities to tree
        for label, label_entities in sorted(entities_by_label.items()):
            label_branch = tree.add(f"[yellow]{label}[/yellow] ({len(label_entities)} found)")
            for entity in label_entities[:10]:
                confidence = entity['score']
                text = entity['text']
                color = "red" if confidence > 0.7 else "yellow" if confidence > 0.5 else "white"
                label_branch.add(f"[{color}]'{text}'[/{color}] (confidence: {confidence:.2f})")
            
            if len(label_entities) > 10:
                label_branch.add(f"[dim]... and {len(label_entities) - 10} more[/dim]")
        
        console.print(tree)
        console.print()


@click.group()
@click.version_option(version="1.0.0")
def cli():
    """GLiNER PII Detection Client - Interact with the GLiNER HTTP server."""
    pass


@cli.command()
@click.option('--server', default=DEFAULT_SERVER, help='Server URL')
def health(server: str):
    """Check server health status."""
    console.print(f"[cyan]Checking server health at {server}...[/cyan]")
    
    response = make_request('GET', '/api/v1/health', server)
    data = response.json()
    
    status_color = "green" if data['status'] == 'healthy' else "red"
    
    console.print()
    console.print(Panel(f"""
[bold]Status:[/bold] [{status_color}]{data['status'].upper()}[/{status_color}]
[bold]Model:[/bold] {data['model_name']}
[bold]Device:[/bold] {data['device']}
[bold]GPU Enabled:[/bold] {'Yes' if data['gpu_enabled'] else 'No'}
[bold]Labels Count:[/bold] {data['labels_count']}
    """.strip(), title="[bold green]Server Health[/bold green]", border_style="green"))


@cli.command()
@click.option('--server', default=DEFAULT_SERVER, help='Server URL')
def info(server: str):
    """Get server information and configuration."""
    console.print(f"[cyan]Fetching server info from {server}...[/cyan]")
    
    response = make_request('GET', '/api/v1/info', server)
    data = response.json()
    
    console.print()
    console.print(Panel(f"""
[bold]Model:[/bold] {data['model_name']}
[bold]Device:[/bold] {data['device']}
[bold]GPU Enabled:[/bold] {'Yes' if data['gpu_enabled'] else 'No'}
[bold]Default Labels:[/bold] {len(data['default_labels'])} labels
[bold]Extended Labels:[/bold] {len(data['extended_labels'])} labels
    """.strip(), title="[bold cyan]Server Information[/bold cyan]", border_style="cyan"))
    
    console.print()
    console.print("[bold]Default Configuration:[/bold]")
    config = data['default_config']
    for key, value in config.items():
        console.print(f"  {key}: {value}")


@cli.command()
@click.argument('file', type=click.Path(exists=True))
@click.option('--server', default=DEFAULT_SERVER, help='Server URL')
@click.option('--labels', help='Comma-separated PII labels')
@click.option('--threshold', type=float, default=0.3, help='Confidence threshold (0.0-1.0)')
@click.option('--chunk-size', type=int, default=8000, help='Characters per chunk')
@click.option('--batch-size', type=int, default=8, help='Chunks to process in parallel')
@click.option('--max-file-size', type=float, default=50.0, help='Maximum file size in MB')
@click.option('--extended-labels', is_flag=True, help='Use extended label set')
@click.option('--output', '-o', type=click.Path(), help='Save results to JSON file')
@click.option('--show-all', is_flag=True, help='Show all files including those without PII')
def upload(
    file: str,
    server: str,
    labels: Optional[str],
    threshold: float,
    chunk_size: int,
    batch_size: int,
    max_file_size: float,
    extended_labels: bool,
    output: Optional[str],
    show_all: bool
):
    """Upload and scan a file for PII."""
    file_path = Path(file)
    file_size_mb = file_path.stat().st_size / (1024 * 1024)
    
    console.print(f"[cyan]Uploading and scanning:[/cyan] {file_path.name} ({file_size_mb:.1f}MB)")
    console.print(f"[cyan]Server:[/cyan] {server}")
    
    # Prepare form data
    form_data = {
        'threshold': str(threshold),
        'chunk_size': str(chunk_size),
        'batch_size': str(batch_size),
        'max_file_size': str(max_file_size),
        'extended_labels': 'true' if extended_labels else 'false'
    }
    
    if labels:
        form_data['labels'] = labels
    
    # Upload file with progress
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        DownloadColumn(),
        TransferSpeedColumn(),
        console=console
    ) as progress:
        task = progress.add_task("[cyan]Uploading and scanning...", total=None)
        
        with open(file_path, 'rb') as f:
            files = {'file': (file_path.name, f)}
            response = make_request('POST', '/api/v1/scan/upload', server, data=form_data, files=files)
        
        progress.update(task, completed=True)
    
    data = response.json()
    
    # Save to file if requested
    if output:
        with open(output, 'w') as f:
            json.dump(data, f, indent=2)
        console.print(f"[green]✓ Results saved to {output}[/green]")
    
    # Display results
    display_scan_results(data, show_all)


@cli.command()
@click.argument('path', type=str)
@click.option('--server', default=DEFAULT_SERVER, help='Server URL')
@click.option('--recursive/--no-recursive', default=True, help='Recursively scan subdirectories')
@click.option('--labels', help='Comma-separated PII labels')
@click.option('--threshold', type=float, default=0.3, help='Confidence threshold (0.0-1.0)')
@click.option('--chunk-size', type=int, default=8000, help='Characters per chunk')
@click.option('--batch-size', type=int, default=8, help='Chunks to process in parallel')
@click.option('--max-workers', type=int, help='Parallel workers for directory scanning')
@click.option('--max-file-size', type=float, default=50.0, help='Maximum file size in MB')
@click.option('--extended-labels', is_flag=True, help='Use extended label set')
@click.option('--output', '-o', type=click.Path(), help='Save results to JSON file')
@click.option('--show-all', is_flag=True, help='Show all files including those without PII')
def scan_path(
    path: str,
    server: str,
    recursive: bool,
    labels: Optional[str],
    threshold: float,
    chunk_size: int,
    batch_size: int,
    max_workers: Optional[int],
    max_file_size: float,
    extended_labels: bool,
    output: Optional[str],
    show_all: bool
):
    """Scan a remote file or directory path for PII."""
    console.print(f"[cyan]Scanning path:[/cyan] {path}")
    console.print(f"[cyan]Server:[/cyan] {server}")
    console.print(f"[cyan]Recursive:[/cyan] {recursive}")
    
    # Prepare request body
    request_body = {
        'path': path,
        'recursive': recursive,
        'threshold': threshold,
        'chunk_size': chunk_size,
        'batch_size': batch_size,
        'max_file_size': max_file_size,
        'extended_labels': extended_labels
    }
    
    if labels:
        request_body['labels'] = [label.strip() for label in labels.split(',')]
    
    if max_workers:
        request_body['max_workers'] = max_workers
    
    # Make request with progress indicator
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        console=console
    ) as progress:
        task = progress.add_task("[cyan]Scanning...", total=None)
        response = make_request('POST', '/api/v1/scan/path', server, json=request_body)
        progress.update(task, completed=True)
    
    data = response.json()
    
    # Save to file if requested
    if output:
        with open(output, 'w') as f:
            json.dump(data, f, indent=2)
        console.print(f"[green]✓ Results saved to {output}[/green]")
    
    # Display results
    display_scan_results(data, show_all)


if __name__ == '__main__':
    cli()


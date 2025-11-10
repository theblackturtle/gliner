#!/usr/bin/env python
"""
Utility for exporting GLiNER checkpoints to ONNX format.

The script downloads the specified GLiNER model, exports it to ONNX, and writes
the associated tokenizer files so they can be consumed by the Rust runtime.

Usage:
    uv run scripts/export_gliner_onnx.py --model urchade/gliner_small-v2.1
"""

import argparse
import json
import sys
from pathlib import Path

import torch
from gliner import GLiNER

DEFAULT_LABELS = [
    "person",
    "email",
    "phone",
    "address",
    "credit card",
    "ssn",
    "account number",
    "password",
    "username",
    "ip address",
    "date",
    "location",
    "organization",
]


def _resolve_output_paths(output_dir: Path, model_name: str) -> dict[str, Path]:
    """Return paths for the exported assets."""
    safe_name = model_name.replace("/", "_")
    model_path = output_dir / f"{safe_name}.onnx"
    tokenizer_dir = output_dir / f"{safe_name}_tokenizer"
    tokenizer_dir.mkdir(parents=True, exist_ok=True)
    return {
        "onnx": model_path,
        "tokenizer_dir": tokenizer_dir,
        "tokenizer_json": tokenizer_dir / "tokenizer.json",
        "config": tokenizer_dir / "config.json",
    }


class _ModelWrapper(torch.nn.Module):
    """Wrap the GLiNER model to normalise the forward signature for ONNX export."""

    def __init__(self, model: torch.nn.Module) -> None:
        super().__init__()
        self.model = model

    def forward(
        self,
        input_ids: torch.Tensor,
        attention_mask: torch.Tensor | None = None,
        token_type_ids: torch.Tensor | None = None,
    ) -> torch.Tensor:
        outputs = self.model(
            input_ids=input_ids,
            attention_mask=attention_mask,
            token_type_ids=token_type_ids,
        )
        logits = getattr(outputs, "logits", None)
        if logits is None:
            msg = "Expected model outputs to expose `logits` attribute"
            raise RuntimeError(msg)
        return logits


def export_model(
    model_name: str,
    output_dir: Path,
    opset: int,
    use_gpu: bool,
    force: bool,
    labels: list[str],
) -> None:
    """
    Perform the model export.

    Args:
        model_name: Hugging Face repository identifier for the GLiNER checkpoint.
        output_dir: Directory to place exported artefacts.
        opset: ONNX opset version.
        use_gpu: Attempt to export with CUDA if available.
        force: Overwrite existing files.
    """
    output_dir.mkdir(parents=True, exist_ok=True)
    paths = _resolve_output_paths(output_dir, model_name)

    if not force and paths["onnx"].exists():
        print(f"ONNX model already exists: {paths['onnx']}")
        return

    print(f"Loading GLiNER model '{model_name}'...")
    scanner = GLiNER.from_pretrained(model_name)

    # The GLiNER wrapper keeps the underlying torch module on `model`.
    torch_model = getattr(scanner, "model", None)
    if torch_model is None:
        msg = "GLiNER object does not expose `.model`; update the exporter."
        raise RuntimeError(msg)

    torch_model.eval()

    device = "cuda" if use_gpu and torch.cuda.is_available() else "cpu"
    torch_model.to(device)
    print(f"Exporting with device={device}, opset={opset}")

    # Prepare dummy inputs for tracing.
    sample_text = (
        "This is a synthetic example sentence used to trace GLiNER for ONNX export."
    )
    encoded = scanner.data_processor.transformer_tokenizer(
        sample_text,
        return_tensors="pt",
        padding="max_length",
        truncation=True,
        max_length=128,
    )
    encoded = {key: value.to(device) for key, value in encoded.items()}

    input_tensors: list[torch.Tensor] = [encoded["input_ids"]]
    input_names = ["input_ids"]

    if "attention_mask" in encoded:
        input_tensors.append(encoded["attention_mask"])
        input_names.append("attention_mask")

    if "token_type_ids" in encoded:
        input_tensors.append(encoded["token_type_ids"])
        input_names.append("token_type_ids")

    output_names = ["logits"]

    dynamic_axes = {
        name: {0: "batch_size", 1: "sequence_length"} for name in input_names
    }
    dynamic_axes["logits"] = {0: "batch_size", 1: "sequence_length"}

    wrapper = _ModelWrapper(torch_model)

    with torch.inference_mode():
        torch.onnx.export(
            wrapper,
            tuple(input_tensors),
            paths["onnx"],
            export_params=True,
            opset_version=opset,
            do_constant_folding=True,
            input_names=input_names,
            output_names=output_names,
            dynamic_axes=dynamic_axes,
            dynamo=False,
        )

    print(f"ONNX model saved to {paths['onnx']}")

    # Persist tokenizer assets for the Rust runtime.
    print("Saving tokenizer artefacts...")
    tokenizer_json = scanner.data_processor.transformer_tokenizer.backend_tokenizer.to_str()
    paths["tokenizer_json"].write_text(tokenizer_json, encoding="utf-8")

    tokenizer_config = {
        "model_name": model_name,
        "max_length": 128,
        "labels": labels,
    }
    paths["config"].write_text(json.dumps(tokenizer_config, indent=2), encoding="utf-8")
    print(f"Tokenizer files written to {paths['tokenizer_dir']}")


def parse_args(argv: list[str]) -> argparse.Namespace:
    """Parse CLI arguments."""
    parser = argparse.ArgumentParser(
        description="Export GLiNER checkpoints to ONNX for the Rust runtime.",
    )
    parser.add_argument(
        "--model",
        type=str,
        default="urchade/gliner_small-v2.1",
        help="Hugging Face model name to export.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("models"),
        help="Directory to store exported artefacts.",
    )
    parser.add_argument(
        "--opset",
        type=int,
        default=17,
        help="ONNX opset version.",
    )
    parser.add_argument(
        "--gpu",
        action="store_true",
        help="Attempt export with CUDA if available.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite existing artefacts.",
    )
    parser.add_argument(
        "--labels",
        type=str,
        help="Comma-separated label list to persist in the exported config.",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> None:
    """Entry point."""
    args = parse_args(argv)
    try:
        export_model(
            model_name=args.model,
            output_dir=args.output,
            opset=args.opset,
            use_gpu=args.gpu,
            force=args.force,
            labels=(
                [label.strip() for label in args.labels.split(",")]
                if args.labels
                else DEFAULT_LABELS
            ),
        )
    except Exception as exc:
        print(f"Export failed: {exc}")
        sys.exit(1)


if __name__ == "__main__":
    main(sys.argv[1:])


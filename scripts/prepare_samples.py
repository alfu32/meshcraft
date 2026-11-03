#!/usr/bin/env python3
"""Compress the Store demonstration projects into the samples embedded in the app.

Thumbnails are captured separately by verify_home_screen.py --thumbnails, because
they require the real renderer.
"""
import argparse
import gzip
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "packaging/microsoft-store/captures/projects"
TARGET = ROOT / "assets/samples"
SAMPLES = ("instrument", "kitchen", "residence", "casework", "rooftop",
           "park", "loft", "plantroom", "studio")


def compressed(path):
    # mtime=0 and an empty filename keep the archive reproducible.
    return gzip.compress(path.read_bytes(), 9, mtime=0)


def expected():
    entries = {}
    for name in SAMPLES:
        data = (SOURCE / f"{name}.octd").read_bytes()
        if b"/home/" in data or b"/Users/" in data or b"\\Users\\" in data:
            raise ValueError(f"{name}: sample contains a private local path")
        entries[name] = hashlib.sha256(data).hexdigest()
    return entries


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Fail if embedded samples are stale")
    args = parser.parse_args()
    manifest = TARGET / "manifest.json"
    entries = expected()
    stale = [
        name for name in SAMPLES
        if not (TARGET / f"{name}.octd.gz").exists()
        or gzip.decompress((TARGET / f"{name}.octd.gz").read_bytes())
        != (SOURCE / f"{name}.octd").read_bytes()
    ]
    if args.check:
        if stale:
            raise SystemExit(f"Stale embedded samples: {stale}")
        return
    TARGET.mkdir(parents=True, exist_ok=True)
    for name in stale:
        (TARGET / f"{name}.octd.gz").write_bytes(compressed(SOURCE / f"{name}.octd"))
    manifest.write_text(json.dumps(
        {"source": str(SOURCE.relative_to(ROOT)), "sha256": entries}, indent=2) + "\n")


if __name__ == "__main__":
    main()

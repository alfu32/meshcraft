#!/usr/bin/env python3
"""Generate narration clips with the local speech model; finished clips are never redone.

Reads the lines the last dry run of build.py wrote, and resumes after an interruption.
"""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import subtitles  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
VOICE = ROOT / "docs/tutorials/voice"
LINES = ROOT / "target/tutorial-work/lines"


def duration(path):
    return float(subprocess.check_output(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)],
        text=True))


def peak_db(path):
    result = subprocess.run(["ffmpeg", "-hide_banner", "-i", str(path), "-af", "volumedetect", "-f", "null", "-"],
                            capture_output=True, text=True)
    found = re.search(r"max_volume: (-?[\d.]+) dB", result.stderr)
    return float(found.group(1)) if found else -200.0


def generate(text, engine, speaker, frames):
    with tempfile.TemporaryDirectory() as directory:
        wav = Path(directory) / "line.wav"
        result = subprocess.run(
            ["./speak.sh", subtitles.spoken(text), str(wav), speaker], cwd=engine,
            env=dict(os.environ, TTS_MAX_FRAMES=str(frames)), capture_output=True, text=True)
        log = result.stdout + result.stderr
        if result.returncode or not wav.exists():
            raise RuntimeError(f"Speech failed for {text!r}: {log[-400:]}")
        if "truncat" in log.lower():
            raise RuntimeError(f"Speech hit the frame limit for {text!r}")
        if peak_db(wav) < -40:
            raise RuntimeError(f"Speech is silent for {text!r}")
        key = subtitles.voice_key(text, speaker)
        clip = VOICE / f"{key}.flac"
        subprocess.run(["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-i", str(wav),
                        "-c:a", "flac", str(clip)], check=True)
        return key, duration(clip)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", type=Path, default=Path.home() / "Development/ollama")
    parser.add_argument("--speaker", default="ryan")
    parser.add_argument("--frames", type=int, default=512)
    args = parser.parse_args()
    lines = []
    for path in sorted(LINES.glob("*.json")):
        lines += json.loads(path.read_text())
    VOICE.mkdir(parents=True, exist_ok=True)
    index_path = VOICE / "index.json"
    index = json.loads(index_path.read_text()) if index_path.exists() else {}
    wanted = {subtitles.voice_key(text, args.speaker): text for text in lines}
    missing = [text for key, text in wanted.items() if key not in index]
    print(f"{len(wanted)} lines, {len(missing)} to generate", flush=True)
    for number, text in enumerate(missing, 1):
        key, seconds = generate(text, args.engine, args.speaker, args.frames)
        index[key] = dict(text=text, spoken=subtitles.spoken(text), speaker=args.speaker,
                          seconds=round(seconds, 3))
        index_path.write_text(json.dumps(index, indent=1, sort_keys=True) + "\n")
        print(f"{number}/{len(missing)} {seconds:.1f}s {text}", flush=True)


if __name__ == "__main__":
    main()

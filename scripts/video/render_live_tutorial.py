#!/usr/bin/env python3
"""Render live MCP chapters with English subtitles and optional narration clips."""

import argparse
import json
from pathlib import Path
import subprocess
import wave

import subtitles


def render(directory, audio_directory=None):
    directory = directory.resolve()
    data = json.loads((directory / "chapters.json").read_text())
    cues, labels, manifest, audio = [], [], [], []
    start = 0.0
    for index, chapter in enumerate(data["chapters"], 1):
        voice = None
        if audio_directory is not None:
            source = audio_directory / f"{index:02}.wav"
            if not source.is_file():
                raise ValueError(f"Missing narration clip: {source}")
            with wave.open(str(source)) as recording:
                if (recording.getframerate(), recording.getnchannels(), recording.getsampwidth()) != (24000, 1, 2):
                    raise ValueError(f"Narration must be mono 24 kHz PCM16: {source}")
                voice = recording.readframes(recording.getnframes())
        spoken = len(voice) / 48000 if voice is not None else 0
        frames = chapter["frames"]
        duration = max(subtitles.reading_seconds(chapter["text"]) + 0.8,
                       spoken + 0.8, len(frames) * 0.18)
        cues.append(dict(start=round(start, 3), end=round(start + duration, 3),
                         text=chapter["text"]))
        labels.append(dict(start=start, end=start + duration, text=chapter["title"]))
        # Preserve the action sequence; let the finished state remain visible.
        per_frame = min(0.45, duration / len(frames))
        for position, frame in enumerate(frames):
            hold = duration - per_frame * (len(frames) - 1) if position == len(frames) - 1 else per_frame
            manifest.extend([f"file '{frame}'", f"duration {hold:.6f}"])
        if voice is not None:
            audio.append((start + 0.2, voice))
        start += duration
    problems = subtitles.audit(cues)
    if problems:
        raise ValueError("Subtitle audit: " + "; ".join(problems))
    manifest.append(f"file '{data['chapters'][-1]['frames'][-1]}'")
    (directory / "frames.concat").write_text("\n".join(manifest) + "\n")
    (directory / "english.srt").write_text(subtitles.srt(cues))
    (directory / "english.vtt").write_text(subtitles.vtt(cues))
    viewport = dict(x=0, y=0, width=1280, height=720)
    (directory / "english.ass").write_text(subtitles.ass(cues, [], [], viewport, keys=labels, size=(1280, 720)))
    (directory / "timeline.json").write_text(json.dumps(dict(
        duration=round(start, 3), narration=bool(audio), cues=cues,
        endpoint=data["endpoint"], app_version=data["app_version"]), indent=2) + "\n")
    command = ["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-f", "concat", "-safe", "0",
               "-i", "frames.concat"]
    if audio:
        track = bytearray((int(start * 24000) + 1) * 2)
        for offset, voice in audio:
            begin = int(offset * 24000) * 2
            track[begin:begin + len(voice)] = voice
        with wave.open(str(directory / "narration.wav"), "wb") as recording:
            recording.setparams((1, 2, 24000, 0, "NONE", "not compressed"))
            recording.writeframes(track)
        command += ["-i", "narration.wav"]
    command += ["-vf", "scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2:color=0x17212b,setsar=1,ass=english.ass,format=yuv420p",
                "-t", f"{start:.3f}", "-r", "30", "-c:v", "libx264", "-preset", "veryfast",
                "-crf", "20", "-threads", "2", "-movflags", "+faststart"]
    if audio:
        command += ["-c:a", "aac", "-b:a", "128k"]
    else:
        command += ["-an"]
    name = "side-table-narrated.mp4" if audio else "side-table-subtitled-preview.mp4"
    command.append(name)
    subprocess.run(command, cwd=directory, check=True)
    print(f"Created {directory / name}: {start:.2f} seconds, {len(cues)} subtitle cues, narration={bool(audio)}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--audio-dir", type=Path, help="Eight mono PCM16/24kHz clips, 01.wav through 08.wav")
    args = parser.parse_args()
    render(args.directory, args.audio_dir.resolve() if args.audio_dir else None)


if __name__ == "__main__":
    main()

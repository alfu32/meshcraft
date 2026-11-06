#!/usr/bin/env python3
"""Edit narration into a recorded tutorial with FilmCraft's headless MCP server.

Each clip goes on A1 at its subtitle's start and FilmCraft exports the mix at a loudness target.
ffmpeg then joins that sound to the recorded picture without re-encoding it, plus the subtitles as
a selectable track, so narration costs no picture quality and subtitles are never burned in.
"""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from director import media_duration

ROOT = Path(__file__).resolve().parents[2]
TUTORIALS = ROOT / "docs/tutorials"
WORK = ROOT / "target/tutorial-work"
LOUDNESS = -16.0


class FilmCraft:
    def __init__(self):
        self.process = subprocess.Popen(["filmcraft-cli", "mcp"], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, text=True, bufsize=1)
        self.next_id = 0
        self._request("initialize", dict(protocolVersion="2025-06-18", capabilities={},
                                         clientInfo=dict(name="meshcraft-tutorials", version="1")))
        self._send(dict(jsonrpc="2.0", method="notifications/initialized"))

    def _send(self, message):
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def _request(self, method, params):
        self.next_id += 1
        self._send(dict(jsonrpc="2.0", id=self.next_id, method=method, params=params))
        while True:
            line = self.process.stdout.readline()
            if not line:
                raise RuntimeError("FilmCraft closed the session")
            reply = json.loads(line)
            if reply.get("id") == self.next_id:
                if "error" in reply:
                    raise RuntimeError(f"{method}: {reply['error']}")
                return reply["result"]

    def tool(self, name, **arguments):
        result = self._request("tools/call", dict(name=name, arguments=arguments))
        text = "".join(c.get("text", "") for c in result.get("content", []))
        if result.get("isError"):
            raise RuntimeError(f"{name} {arguments}: {text[:600]}")
        try:
            return result.get("structuredContent") or json.loads(text)
        except json.JSONDecodeError:
            return text

    def run(self, command, **params):
        return self.tool("command_run", id=command, params=params)

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=30)


def items_of(imported):
    found = imported.get("imported") or imported.get("items") or imported.get("ids")
    if not found:
        raise RuntimeError(f"Import returned no items: {imported}")
    return [i["id"] if isinstance(i, dict) else i for i in found]


def assemble(video, cues, output, project=None):
    """cues: list of (start seconds, clip path). Writes a video with picture, sound and subtitles."""
    subtitles_file = video.with_suffix(".srt")
    mix = output.with_name(output.stem + ".mix.flac")
    film = FilmCraft()
    try:
        picture = items_of(film.tool("media_import", paths=[str(video)]))[0]
        film.run("file.newSequence", name=video.stem, width=1920, height=1080, fps=30.0)
        film.run("timeline.place", item=picture, track="V1", seconds=0.0, audio=False)
        film.run("captions.import", path=str(subtitles_file), name="English", language="en")
        for start, clip in cues:
            item = items_of(film.tool("media_import", paths=[str(clip)]))[0]
            film.run("timeline.place", item=item, track="A1", seconds=start, video=False)
        if project:
            film.run("file.saveAs", path=str(project))
        film.run("file.exportMedia", path=str(mix), format="flac", audio=True,
                 loudnessLufs=LOUDNESS, wait=True)
    finally:
        film.close()
    subprocess.run(
        ["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-i", str(video), "-i", str(mix),
         "-i", str(subtitles_file), "-map", "0:v", "-map", "1:a", "-map", "2:s", "-c:v", "copy",
         "-c:a", "aac", "-b:a", "96k", "-c:s", "mov_text", "-metadata:s:s:0", "language=eng",
         "-disposition:s:0", "default", "-movflags", "+faststart", str(output)], check=True)
    mix.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--only", help="recipe name prefix")
    args = parser.parse_args()
    videos = sorted((TUTORIALS / "videos").glob("[0-9]*.cues.json"))
    if args.only:
        videos = [v for v in videos if v.name.startswith(args.only)]
    for path in videos:
        data = json.loads(path.read_text())
        cues = [(c["start"], TUTORIALS / "voice" / f"{c['voice']}.flac") for c in data["cues"] if c.get("voice")]
        missing = [c["text"] for c in data["cues"] if not c.get("voice")]
        if missing:
            sys.exit(f"{path.name}: no voice for {missing[:3]}; run voice.py, then re-record")
        name = data["video"].removesuffix(".mp4")
        source = TUTORIALS / "videos" / data["video"]
        output = source.with_name(name + ".narrated.mp4")
        WORK.joinpath(name).mkdir(parents=True, exist_ok=True)
        assemble(source, cues, output, WORK / name / f"{name}.fcproj")
        output.replace(source)
        data["duration"] = round(media_duration(source), 3)
        path.write_text(json.dumps(data, indent=2) + "\n")
        print(f"{name}: {len(cues)} clips, {source.stat().st_size / 1e6:.1f} MB", flush=True)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Create two sub-15-minute editions of tutorial 7, preserving the original."""

import hashlib
import json
from pathlib import Path
import subprocess

from director import ROOT, media_duration
import subtitles

VIDEO = ROOT / 'docs/tutorials/videos/07-office-floor.mp4'
CUT = 26399 / 30  # 14:39.966667: source IDR frame, after the stair openings.
CARD = 4.0
WORK = ROOT / 'target/tutorial-work/office-split'
OUT = VIDEO.parent


def run(*args):
    subprocess.run(['ffmpeg', '-y', '-hide_banner', '-loglevel', 'error', *map(str, args)], check=True)


def card(name, title, subtitle):
    ass = WORK / (name + '.ass')
    ass.write_text(subtitles.overlay([], [dict(start=0, end=CARD, title=title, subtitle=subtitle)],
                                     dict(x=0, y=0, width=1920, height=1080)))
    output = WORK / (name + '.mp4')
    run('-f', 'lavfi', '-i', 'color=c=0x20262e:s=1920x1080:r=30:d=4',
        '-vf', f"ass='{ass}',format=yuv420p", '-c:v', 'libx264', '-preset', 'slow',
        '-crf', '24', '-profile:v', 'high', '-level:v', '5.0', '-g', '150',
        '-video_track_timescale', '15360', '-an', output)
    return output


def main():
    WORK.mkdir(parents=True, exist_ok=True)
    original_hash = hashlib.sha256(VIDEO.read_bytes()).hexdigest()
    source = json.loads(VIDEO.with_suffix('.cues.json').read_text())
    total = media_duration(VIDEO)
    voices = json.loads((ROOT / 'docs/tutorials/voice/index.json').read_text())
    # No narration clip crosses the cut; there is silence on both sides.
    assert not any(c['start'] < CUT < c['start'] + voices[c['voice']]['seconds'] for c in source['cues'])
    assert not any(c['start'] < CUT < c['end'] for c in source['cues'])
    closing = card('part-1-closing', 'End of part 1', 'Continue with part 2: doors, facades and ceiling')
    opening = card('part-2-opening', 'An office floor · Part 2', 'Doors, facades and ceiling · continues part 1')
    reports = []
    for number, start, end, lead, tail in [(1, 0, CUT, 0, CARD), (2, CUT, total, CARD, 0)]:
        name = f'07-office-floor-part-{number}'
        body = WORK / (name + '-body.mp4')
        # A keyframe boundary lets both source picture segments be copied losslessly.
        # The frame cap excludes the next GOP despite H.264 decoder reordering delay.
        run('-ss', start, '-i', VIDEO, '-t', end - start, '-map', '0:v:0', '-c:v', 'copy',
            '-frames:v', round((end - start) * 30), '-an', body)
        segments = [(body, end - start), (closing, CARD)] if number == 1 else [(opening, CARD), (body, end - start)]
        manifest = WORK / (name + '.concat')
        manifest.write_text(''.join(f"file '{p}'\nduration {seconds:.9f}\n" for p, seconds in segments))
        picture = WORK / (name + '-picture.mp4')
        run('-f', 'concat', '-safe', '0', '-i', manifest, '-map', '0:v:0', '-c:v', 'copy', '-an', picture)
        cues = [dict(c, start=round(c['start'] - start + lead, 3), end=round(c['end'] - start + lead, 3))
                for c in source['cues'] if start <= c['start'] < end]
        srt = OUT / (name + '.srt')
        srt.write_text(subtitles.srt(cues))
        (OUT / (name + '.vtt')).write_text(subtitles.vtt(cues))
        duration = media_duration(picture)
        output = OUT / (name + '.mp4')
        # Trim the original soundtrack and add only silence for the new cards.
        # The speech clips are reused; no voice generation or speed change occurs.
        audio = f'atrim=start={start:.9f}:end={end:.9f},asetpts=PTS-STARTPTS'
        if lead:
            audio += f',adelay={round(lead * 1000)}:all=1'
        audio += f',apad,atrim=duration={duration:.9f}'
        run('-i', picture, '-i', VIDEO, '-i', srt, '-map', '0:v:0', '-map', '1:a:0', '-map', '2:s:0',
            '-filter:a', audio, '-c:v', 'copy', '-c:a', 'aac', '-b:a', '128k', '-c:s', 'mov_text',
            '-metadata:s:s:0', 'language=eng', '-disposition:s:0', 'default',
            '-t', duration, '-movflags', '+faststart', output)
        measured = media_duration(output)
        assert measured < 900, measured
        metadata = dict(source, video=output.name, duration=round(measured, 3), cues=cues,
                        source_video=VIDEO.name, source_sha256=original_hash,
                        source_range_seconds=[start, end], leading_card_seconds=lead, trailing_card_seconds=tail)
        (OUT / (name + '.cues.json')).write_text(json.dumps(metadata, indent=2) + '\n')
        reports.append(dict(video=output.name, duration_seconds=measured, cues=len(cues),
                            bytes=output.stat().st_size, leading_card_seconds=lead, trailing_card_seconds=tail,
                            source_range_seconds=[start, end]))
        print(json.dumps(reports[-1]), flush=True)
    assert hashlib.sha256(VIDEO.read_bytes()).hexdigest() == original_hash
    (WORK / 'export.json').write_text(json.dumps(dict(original_sha256=original_hash, cut_seconds=CUT, parts=reports), indent=2) + '\n')


if __name__ == '__main__':
    main()

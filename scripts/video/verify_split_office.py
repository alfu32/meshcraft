#!/usr/bin/env python3
"""Verify the two tutorial 7 editions against the preserved original."""

from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess

from director import ROOT
import subtitles

VIDEO = ROOT / 'docs/tutorials/videos'
WORK = ROOT / 'target/tutorial-work/office-split'
REVIEW = ROOT / 'docs/tutorials/review'


def probe(path):
    return json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-show_streams',
                                              '-show_format', '-of', 'json', str(path)], text=True))


def hashes(name):
    output = WORK / (name + '.framemd5')
    fingerprint = output.with_suffix('.frames-source.sha256')
    checksum = hashlib.sha256((VIDEO / (name + '.mp4')).read_bytes()).hexdigest()
    if not output.exists() or not fingerprint.exists() or fingerprint.read_text().strip() != checksum:
        subprocess.run(['ffmpeg', '-y', '-v', 'error', '-threads', '2', '-i', str(VIDEO / (name + '.mp4')),
                        '-map', '0:v:0', '-fps_mode', 'passthrough', '-f', 'framemd5', str(output)], check=True)
        fingerprint.write_text(checksum + '\n')
    return [line.rsplit(',', 1)[-1].strip() for line in output.read_text().splitlines()
            if line and not line.startswith('#')]


def frame(video, time, output):
    subprocess.run(['ffmpeg', '-y', '-v', 'error', '-ss', str(time), '-i', str(video),
                    '-sn', '-frames:v', '1', str(output)], check=True)


def main():
    WORK.mkdir(parents=True, exist_ok=True)
    REVIEW.mkdir(exist_ok=True)
    names = ['07-office-floor', '07-office-floor-part-1', '07-office-floor-part-2']
    original = {}
    for suffix in ['.mp4', '.cues.json', '.srt', '.vtt']:
        path = VIDEO / (names[0] + suffix)
        expected = subprocess.check_output(['git', 'rev-parse', '0.57.1:' + str(path.relative_to(ROOT))], text=True).strip()
        actual = subprocess.check_output(['git', 'hash-object', str(path)], text=True).strip()
        assert expected == actual, f'Original changed: {path.name}'
        original[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
    with ThreadPoolExecutor(max_workers=3) as pool:
        decoded = list(pool.map(hashes, names))
    cut_frame = 26399
    assert decoded[1][:-120] == decoded[0][:cut_frame], 'Part 1 picture differs from source'
    assert decoded[2][120:] == decoded[0][cut_frame:], 'Part 2 picture differs from source'
    print('Every source picture frame retained in order, without loss or duplication.', flush=True)
    source = json.loads((VIDEO / (names[0] + '.cues.json')).read_text())
    combined, reports = [], []
    for number, name in enumerate(names[1:], 1):
        path = VIDEO / (name + '.mp4')
        data = json.loads(path.with_suffix('.cues.json').read_text())
        p = probe(path)
        duration = float(p['format']['duration'])
        assert 0 < duration < 900
        assert [s['codec_name'] for s in p['streams']] == ['h264', 'aac', 'mov_text']
        assert (p['streams'][0]['width'], p['streams'][0]['height'], p['streams'][0]['avg_frame_rate']) == (1920, 1080, '30/1')
        assert abs(duration - data['duration']) < .002
        assert not subtitles.audit(data['cues'])
        assert all(0 <= c['start'] < c['end'] <= duration for c in data['cues'])
        combined.extend((c['text'], c['voice']) for c in data['cues'])
        loudness = subprocess.run(['ffmpeg', '-hide_banner', '-i', str(path), '-map', '0:a', '-af',
                                   'ebur128', '-f', 'null', '-'], capture_output=True, text=True, check=True)
        loudness = loudness.stderr.rsplit('Summary:', 1)[-1].strip()
        player = subprocess.run(['mpv', '--no-config', '--no-audio', '--vo=null', '--frames=1',
                                 '--sub-auto=exact', str(path)], capture_output=True, text=True, check=True)
        tracks = [line for line in (player.stdout + player.stderr).splitlines() if 'Subs' in line]
        assert sum('●' in line for line in tracks) == 1, tracks
        samples = {'opening': 1.5, 'ending': duration - (3 if number == 2 else 1.5)}
        samples.update({'join': duration - 4.15} if number == 1 else {'join': 4.15})
        for label, time in samples.items():
            frame(path, time, REVIEW / (name + '-' + label + '.png'))
        reports.append(dict(video=path.name, duration_seconds=duration, cues=len(data['cues']),
                            bytes=path.stat().st_size, codecs=[s['codec_name'] for s in p['streams']],
                            decoded_frames=len(decoded[number]), source_frames=len(decoded[number]) - 120,
                            loudness=loudness, player_subtitles=tracks, frame_samples=samples))
    assert combined == [(c['text'], c['voice']) for c in source['cues']]
    result = dict(original_files_unchanged=original, cut_seconds=cut_frame / 30,
                  source_frames=len(decoded[0]), source_frame_continuity='Every decoded source frame matches exactly, in order',
                  narration_lines_unchanged=len(combined), parts=reports, audio_listened_to=False)
    (REVIEW / '07-office-floor-parts-checks.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2), flush=True)


if __name__ == '__main__':
    main()

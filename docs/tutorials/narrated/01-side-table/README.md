# Side table tutorial: live MCP capture

Recorded from the existing Meshcraft 0.52.0 session at
`http://127.0.0.1:8766/mcp` on November 5, 2026. No second app was launched.
The previously saved empty document was saved under `side-table.octd` before
modeling; its original file was not overwritten.

`side-table-subtitled-preview.mp4` is a 62.87-second, 1280×720 H.264 preview
with English subtitles and chapter labels. **It has no narration yet.**
The configured OpenAI API credential was rejected with HTTP 401
`invalid_api_key`. No credentials or authentication responses are stored here.

The video animates 83 genuine viewport captures taken between live modeling
operations, rather than recording the full desktop UI. Source frames are in
`frames/`; `mcp-actions.json` records the MCP requests. `chapters.json` is the
editable narration script and visual sequence. `english.srt`, `english.vtt`
and `english.ass` contain the same eight English cues.

The saved model contains one tabletop and four leg occurrences sharing one
definition. The top is 60×60×3 cm, with its upper surface at 45 cm. The legs
are 4×4×42 cm. `scene.json` records the inspected final scene.

To render again from the captured frames:

```bash
python3 scripts/video/render_live_tutorial.py docs/tutorials/narrated/01-side-table
```

To finish with narration, supply eight mono, PCM16, 24 kHz WAV files named
`01.wav` through `08.wav`, corresponding to the eight chapter texts. Then run:

```bash
python3 scripts/video/render_live_tutorial.py docs/tutorials/narrated/01-side-table --audio-dir /absolute/path/to/clips
```

The renderer measures each clip and extends its visual chapter and subtitle
duration to accommodate it. It adds silence between chapters and writes the
combined `narration.wav` and `side-table-narrated.mp4`.

Validation: five existing subtitle tests passed; subtitle timing audit passed;
ffprobe confirmed H.264, 1280×720, 30 fps and 62.87 seconds. A decoded frame
was visually inspected for readable labels and subtitles, and a final live
capture was inspected for the completed table. Audio rendering remains
unverified until narration is supplied.

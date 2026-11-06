# Meshcraft tutorial videos

Tutorials recorded from the real application at a pace a person can follow.

| # | Video | Teaches |
| --- | --- | --- |
| 1 | [A side table](videos/01-side-table.mp4) | Units, grid snapping, axis helpers, Rectangle, Push/Pull, Make Object, Move, Move Copy, typed distances |
| 2 | [A small cabin](videos/02-small-cabin.mp4) | The Architecture template, Slab, Wall chain, helpers on a wall, Add Hole, Door, Window, roof |
| 3 | [Materials](videos/03-materials.mp4) | New Wood, New Grass, Apply Material, per-instance materials |
| 4 | [A parametric bookcase](videos/04-parametric-bookcase.mp4) | Script objects, a grid helper on a face, dragging a hotspot along it, instance parameters |
| 5 | [A round pavilion](videos/05-round-pavilion.mp4) | Circle, axis helpers, Rotate Copy with a repeat count, shared definitions |
| 6 | [A curtain wall with no code](videos/06-curtain-wall.mp4) | A dynamic object from hotspots alone: a Stretch handle for size and a Multiply Linear handle that lays out the wall |
| 7.1 | [Office floor — Part 1 · 14:44](videos/07-office-floor-part-1.mp4) | Structural frame and hotspot array, stairs, lifts, core partitions and initial openings |
| 7.2 | [Office floor — Part 2 · 13:33](videos/07-office-floor-part-2.mp4) | Remaining door openings and frames, labels, repeating glazed facades and ceiling panels |
| 7 | [Original full office floor · 28:09](videos/07-office-floor.mp4) | A 6 × 6 × 3.6 m frame and volumetric hotspot array, central stair/lift core with clear approaches and connected return flights, repeated partitions, doors, glazed facades and ceiling panels |

Each video has narration and a selectable English subtitle track; `.srt` and `.vtt` copies sit beside
it. Subtitles are never burned in. A player that auto-loads the sidecar still shows only one set.
The saved models are in [projects](projects/), and the [recipes](recipes/) are the source of each video.

Agents recording a new tutorial should follow the [authoring guide](AUTHORING.md).

## How the videos are made

```sh
cargo build --locked -p meshcraft-desktop --bin meshcraft
python3 scripts/video/build.py --dry-run     # check every recipe; also lists the narration lines
python3 scripts/video/voice.py               # generate missing narration with the local model
python3 scripts/video/build.py               # record, paced by the narration
python3 scripts/video/assemble.py            # edit the narration in with FilmCraft
python3 -m unittest discover -s scripts/video -p 'test_*.py'
```

Everything runs on the local machine, in a private Xvfb display that leaves the desktop alone. It
needs `Xvfb`, `xdotool`, `xsetroot`, the Mesa lavapipe Vulkan driver, `ffmpeg` with libass and libx264,
`filmcraft-cli` for assembly, and the local speech engine (`voice.py --engine`, which runs its `speak.sh`).

A dry run takes about two minutes per recipe. Generating speech takes about 7.5 s per second of
narration. Clips are cached in [voice](voice/) by what is said, so re-recording never regenerates them
and an interrupted run resumes. Assembly takes seconds, because the picture is copied rather than re-encoded,
and running it again replaces the sound rather than adding to it.

## Rules for recipes

These came from reviewing the first recordings, and they apply to every tutorial.

- **Start from the beginning.** A new project opens by setting the unit name, the unit size and the grid
  size in Model Settings, or by choosing a Home template. A recipe that continues an earlier project says so.
- **Work in the template's units.** The Architecture template makes one unit 30 cm, and its defaults (walls
  1 thick and 9 high, offset 0.5) belong to that scale. Size the model in those units; do not override them.
- **Pick points the way a user does.** `d.pick` and `d.aim` use MCP's native pointer, which runs the mouse's own
  handlers, and stop the build unless the app reports a snap at exactly that point and of that kind.
- **Never pick in mid-air.** Go upward along an axis helper (`d.helper`, `T`), and drag on a grid helper placed
  on a face (`d.grid_helper_on_face`, `G`). A grid helper reaches 12 grid steps from where it is placed, so choose
  a grid size that covers the target. Openings and frames are picked on the wall surface.
- **Attach what a window misses.** A selection window takes only what lies wholly inside it. For a Stretch handle,
  add the corners of faces that cross the window, or those faces stay behind. With two hotspots selected, Attach
  goes to the older one, so place the driving handle first.
- **Typing is the secondary way.** Show a typed value as an alternative, not as the main route.
- **Show the tools working.** The 3D cursor shows the snap, the video labels the kind the app reported, and
  `d.window` draws the real selection window. A window would also take anything else inside it, so use `d.select`
  where that would happen.
- **Check the model, not the script.** A claim in a line of narration needs a `d.require` behind it. Those
  checks caught false lines, such as materials reaching every instance when they apply per instance.
- **Pace for a person.** Each line stays up for at least its reading time at 14 characters per second, or
  for its spoken length if that is longer. It stays until the next line replaces it, up to nine seconds beyond that.
- **Season lightly.** Add a "nice", "ooh" or "niiiice!" every few lines, and an "uh oh" only where something
  is really missing. Keep it friendly, not breathy.

The recipes work through MCP (see [the native pointer](../mcp.md#native-pointer-the-3d-cursor-picks-and-the-selection-window)).
Only the visible arrow, the settings fields, the camera orbit, preview wheel zoom, ribbon tab clicks and closing
editors use real X input. Add a pronunciation to `SPOKEN` in `scripts/video/subtitles.py` when the written form reads
badly, such as `Push/Pull`.

## Known limits

- Rendering is software only under Xvfb, so tool previews trail the cursor by a few tenths of a second. `d.aim`
  pauses before each click so the preview lands first. A real GPU would not show this lag.
- The cursor is an arrow in every state. Without a full cursor theme, Xvfb shows the root cursor, which the
  build sets to an arrow, so the videos do not show the crosshair a desktop shows during a tool.
- The Save button in the material editor has no measured position, so `03-materials.py` holds a fixed
  coordinate. The button position only aims the cursor, and the save itself goes through MCP.

Tutorial 7 also has two editions under 15 minutes. Recreate them with
`python3 scripts/video/split_office.py`; this preserves the original video and its sidecars,
copies the source picture without re-encoding, and retimes the selectable subtitles.
The existing narration is retained, with silence during the added part cards.

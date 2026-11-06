# Tutorial 7 revision: structural office floor

The owner's revised drawings are preserved in [tutorial-07-plan.md](tutorial-07-plan.md).
This revision replaces the 27 × 15 m example with a 42 × 18 m office floor.

## Geometry and workflow

- Architecture template: 30 cm per model unit; half-unit grid (15 cm).
- Slab: 140 × 60 units, corresponding to 42 × 18 m.
- Structural prototype: four corner columns and four perimeter beams, 20 × 20 units in plan and 12 units high (6 × 6 × 3.6 m).
- A user-defined Multiply Volumetric hotspot generates seven modules across and three deep. The instance handle is dragged to the opposite slab corner. Expected result: 21 modules, 2,016 structural triangles.
- Axes A–H run along X at 20-unit intervals; rows 1–4 run along Z at 20-unit intervals.
- Central three bays C2–F3 contain the two protected evacuation stairs and the four-lift bank, with two lift cases on each side of its central hall.
- Partition thickness: 0.5 unit (15 cm); plain east and west side walls: 1 unit (30 cm), without windows.
- North and south facades use a user-defined repeating glazed panel with only left and right vertical framing. No opaque top rail.
- FR30/FR90 labels record the owner's design intent. The model does not establish a tested assembly's fire resistance.

The two source drawings differ at F2–G3: the standard drawing marks a meeting room, while the detailed drawing places storage, copier and kitchen on the right. The recipe follows the detailed service-room arrangement in F–G and places the meeting room in the spare G–H bay, retaining both programs. This assumption was reported during authoring.

## Source models

The structural array has passed a native pointer replay. Its durable checkpoint is
[07-office-floor-structure.octd](projects/07-office-floor-structure.octd).
The [complete floor](projects/07-office-floor.octd) passes the native recipe and save/reopen checks. The [revised narrated video](videos/07-office-floor.mp4) is complete (28 min 28 s, 47 cues), with selectable subtitles and [review evidence](review/07-office-floor-checks.json). The earlier example remains available in [release 0.56.0](https://github.com/alfu32/meshcraft/releases/tag/0.56.0).

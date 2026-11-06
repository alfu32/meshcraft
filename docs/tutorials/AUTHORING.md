# Recording a new tutorial: guide for agents

This guide takes you from a topic to a released, narrated tutorial. The six existing videos were reviewed by the
project owner, and these rules come from that review. Read the whole guide before you start.
[README.md](README.md) lists the videos and the short form of the rules.

## What good looks like

A viewer should be able to follow along in the real app and get the same result. So:

- **Real workflow.** Nothing is picked from thin air. Every point comes from something the cursor snaps to.
- **Human pace.** The owner approved the current pace. Don't speed it up.
- **Starts at the beginning.** Units or a template come first.
- **Ends properly.** Finish on the completed model and a closing card, never a cut.
- **True narration.** Each line describes what the app really does, and a check in the recipe backs it up.

## Rules from the owner

Treat these as requirements. Each one corrects something that went wrong in an earlier recording.

1. **Set the units first.** Either type the unit name, unit size and grid size into Model Settings (`d.units`), or pick a
   Home template. A recipe that continues an earlier project says so in its first line.
2. **Work in the template's units.** In the Architecture template one unit is 30 cm, and its defaults belong to that
   scale: walls 1 thick and 9 high, offset 0.5, slabs 1 thick. Never override them to suit metres. Size the model in
   units: a 9 × 6 m building is 30 × 20.
3. **Never pick in mid-air.** To go upward, place an axis helper with `T` and pick along it. To drag or pick on a
   vertical plane, place a grid helper with `G` on a face. This also applies to Push/Pull, Move, Copy and hotspot drags.
4. **Pick on real surfaces.** Holes go on the wall surface, and windows go on the wall surface at the hole.
5. **Typing is the secondary way.** Show a typed value as an alternative, at most once or twice. Helpers are the main way.
6. **Show the tools on camera.** The 3D cursor must be seen snapping, and selection windows must be drawn.
7. **Don't burn in subtitles.** They are a selectable track. Burning them in shows two sets in mpv.
8. **Season the narration lightly.** About one "nice", "good", "ok", "cool", "ooh" or "wow" every four lines, and an "uh
   oh" only where something really is missing. Keep it friendly. See [Narration](#narration) for stretched words.

Also remember these lessons from review:

- **Keep the home directory out of videos.** The title bar shows the open path. Copy a project to a neutral temp folder
  before opening it, as `03-materials.py` does.
- **Hold the voice text steady.** Speech is slow to generate, so change a line only when it has to change.

## Setup

Everything runs on the local machine in a private Xvfb display, so it doesn't touch the user's desktop.

- **Tools:** `Xvfb`, `xdotool`, `xsetroot`, the Mesa lavapipe Vulkan driver, and `ffmpeg` with libass and libx264.
- **Editing:** `filmcraft-cli`, which `assemble.py` drives over MCP. Its docs are at
  <https://github.com/storytold/filmcraft/tree/main/docs>.
- **Voice:** the local model in `~/Development/ollama`, reached through its `speak.sh`, with the `ryan` voice. It is the
  only speech engine available. Don't use a cloud voice. Read that folder's README before changing how it's called.

```sh
cargo build --locked -p meshcraft-desktop --bin meshcraft
```

## Workflow

| Step | Command | Time |
| --- | --- | --- |
| 1. Write the recipe | `docs/tutorials/recipes/NN-name.py` | |
| 2. Check it | `python3 scripts/video/build.py --only NN --dry-run` | 1–2 min per run |
| 3. Generate narration | `python3 scripts/video/voice.py` | about 1 min per new line |
| 4. Record | `python3 scripts/video/build.py --only NN` | real time, about 3 min |
| 5. Add the sound | `python3 scripts/video/assemble.py --only NN` | seconds |
| 6. Review | see [Review](#review-before-you-call-it-done) | |
| 7. Release | see [Finishing](#finishing) | |

Details that matter:

- **Bump the version before you record.** The window title is in every frame. If the app needs a fix, make it and
  rebuild first too.
- **The order is fixed.** A dry run writes the narration list that `voice.py` reads. `build.py` then paces the video from
  each clip's real length, so narration can't overrun the action. Change a line, and you repeat steps 2 to 5.
- **`build.py` replaces the video with a silent one,** so always run `assemble.py` after it. Running `assemble.py` twice
  is safe. It stops with an error if a line has no clip.
- **Don't record while anything heavy runs.** Speech generation, cargo and other recordings all add lag. Dry runs and
  documentation are fine at the same time.
- **Clips are cached by what is said** in `docs/tutorials/voice`. Re-recording doesn't regenerate them, and an interrupted
  `voice.py` resumes. Leave unused clips in place.
- **Long commands:** run recording and voice generation in the background.
- **Large exports:** a long recipe can set `d.video_crf = 24` in `setup` (the default is 23).
  Keep 1080p/30 fps and review text and thin lines after encoding. Cue metadata uses the exported media duration.

If a GPU-related reboot interrupts speech generation, first check which cached clips survived.
Resume with `TTS_DEVICE=CPU QWEN_CODEC_CPU=1 python3 scripts/video/voice.py` to use the same local
Ryan model entirely on the CPU. The script generates only missing lines. This recovery path
completed the last five lines of tutorial 7 after a reboot; do not start recording until it exits.

## Starting a recipe

Copy this. It passes a dry run as written, and it is the smallest complete pattern.

```python
"""Tutorial 7: a plinth. One sentence on what this teaches."""

GRID = ("Grid", "Grid Line", "Axis Guide")
ON_HELPER = ("Axis Guide",)
CORNER = ("Endpoint",)


def setup(d):
    # Runs before recording starts, so the viewer never sees it.
    d.command("ui.update_preferences", accordion_settings_panels=True)
    d.command("view.model_settings")
    d.pose((95, 75, 110), (0, 10, 0))


def record(d):
    d.title("A plinth", "Meshcraft tutorials · 7")
    d.say("Units first: centimetres, on a five centimetre grid.")
    d.units("cm", 1, 5)
    d.hold()

    d.say("Choose Rectangle, and click two grid points.")
    d.tool("tool.builtin.rectangle")
    d.pick((-20, 0, -20), GRID)
    d.pick((20, 0, 20), GRID)
    d.escape()
    d.require(d.root()["counts"]["triangles"] == 2, "a flat face")

    d.say("Uh oh, nothing to snap to up in the air. Press T on the corner for a helper.")
    d.helper((-20, 0, 20), CORNER)

    d.say("Push/Pull: pick the corner, then a point up the helper.")
    d.tool("tool.builtin.push_pull", tab="Construction")
    d.pick((-20, 0, 20), CORNER)
    d.pick((-20, 15, 20), ON_HELPER)
    d.escape()
    d.require(d.root()["counts"]["triangles"] == 12, "a solid")

    d.say("Nice. Clear Guides, and the plinth is done.")
    d.tool("view.guides.clear")
    d.finish("Thanks for watching", "Meshcraft tutorials · 7")
```

Coordinates are world model units, with Y up. For a new project, `setup` must leave the panels and camera ready. To start
from a template, open Home in `setup` with `d.command("file.home", open=True)` and copy the opening of `02-small-cabin.py`.

## Director reference

The methods are in `scripts/video/director.py`.

**Narration and pacing**

| Call | Use |
| --- | --- |
| `d.title(title, subtitle)` | Title card; the opening of every video |
| `d.say(text)` | Start a line. It waits for the previous line to finish, and the actions after it happen during it |
| `d.hold()` | Wait until the current line has been read and spoken. Use it after an action the viewer should take in |
| `d.wait(seconds)` | A plain pause |
| `d.finish(title, subtitle)` | Quiet moment on the model, then a closing card. The end of every video |

**Points and helpers.** `kinds` limits which snap is accepted; leave it out to accept any snap at that exact point.

| Call | Use |
| --- | --- |
| `d.aim(point, kinds)` | Place the 3D cursor; stops the build unless it snaps exactly there. Shows the snap's name |
| `d.pick(point, kinds)` | `aim`, then click. Use it for every tool pick |
| `d.helper(point, kinds)` | Axis helper at the cursor, shown as `T` |
| `d.grid_helper_on_face(point, normal)` | Grid helper on a face, shown as `G`; returns the snap |
| `d.drag(start, end, kinds)` | A native drag between two snap points, for hotspots and handles |
| `d.point(point)` | Aim with no snap check. Only for a direction that a typed value will finish |

Snap kinds: `Free`, `Face`, `Line`, `Grid`, `Grid Line`, `Grid Guide`, `Axis Guide`, `Endpoint`, `Midpoint`.

**Selection**

| Call | Use |
| --- | --- |
| `d.click(point, count=1, add=False)` | Click with Select active. It clears first, or adds with `add=True`, and requires something new selected |
| `d.window(corners, margin=26)` | Draw a selection window around world points. Prefer this where it works, because it shows on camera |
| `d.select(targets, around=point)` | Select exact targets over MCP, with the cursor travelling there. For when a window would catch too much |
| `d.selected()` | The current selection targets |

**Tools and settings**

| Call | Use |
| --- | --- |
| `d.tool(command, tab="Home", **args)` | Switch tab, travel to the real ribbon button, run the command. Only for commands on that tab's ribbon |
| `d.tab(name)` | Switch ribbon tab: Home, Construction, Solids, Civil, Materials, Objects & Hotspots, Settings |
| `d.command(command, **args)` | Run a command with no cursor travel, for anything not on the ribbon |
| `d.units(name, size, grid)` | Type the units into the real Model Settings fields. Open that panel in `setup` first |
| `d.field(label, text)` | Type into one settings field |
| `d.type(text)`, `d.enter()`, `d.escape()` | Typed input, Enter and Esc over MCP, shown as keycaps |
| `d.key(name)`, `d.close_modal()` | Real keys for modal editors. `close_modal` shows no keycap, so viewers aren't taught that Esc saves |
| `d.click_mark(x, y)` | Travel to a screen position and mark a click on a control with no measured position; MCP does the work |

**Camera and checks**

| Call | Use |
| --- | --- |
| `d.pose(position, target)` | Set the camera at once |
| `d.fly(position, target)` | Move the camera smoothly |
| `d.orbit(dx, dy)` | A real right-drag. Use it near the end to show the result |
| `d.wheel(x, y, button, clicks)` | Real wheel input, such as zooming an editor preview |
| `d.require(condition, message)` | Stop the build with a message |
| `d.scene()`, `d.root()`, `d.primitives(path, kinds)` | Read objects, the root object and geometry targets |
| `d.rpc.status()`, `d.rpc.call(tool, args)`, `d.rpc.select(targets)` | Direct MCP. See [docs/mcp.md](../mcp.md) |

To find commands, run `target/debug/meshcraft --command-catalog`. Ribbon layout is in `apps/desktop/src/functional_ui.rs`.
Use `TUTORIAL_TRACE=1` on a dry run to print each snap and click. Add `print` calls while debugging, then remove them.

## What will trip you up

Every item here cost a failed run or a bad recording. The app's own status text often explains a failure, so print
`d.rpc.status()["status"]` first.

**Snapping and framing**

- **Dimensions must land on grid steps,** or the build will stop at the pick. Choose them first.
- **A hidden point can't be picked.** Hovering finds what's in front, so a corner behind an object snaps to the object.
  Move the camera. A camera on the diagonal also hides the far leg behind the near one.
- **Keep everything in frame.** A point under a panel fails with "off the viewport", so frame the final model, not the first
  shape. Use `d.fly` when the model grows.
- **Helpers have a reach:** an axis helper snaps up to 64 grid steps from its origin, and a grid helper up to 12. Past
  that, the cursor falls through to the ground far away. Choose the grid size accordingly.
- **Close snaps compete.** `d.aim` nudges a few pixels by itself. A wanted `Face` next to a `Midpoint` or `Grid` needs a
  different spot. A face also offers `Grid` snaps at round coordinates, with the face's normal, which make good helper origins.
- **Blue is up.** A helper placed on the ground points its blue axis upward. Check any colour you name against a frame.

**Selection**

- **Drawing tools leave their result selected,** and that often survives `d.escape()` and modal editors. Use it.
- **Clicking something selected toggles it off,** which is why `d.click` clears first.
- **One click on a face takes one triangle.** A rectangle has two, so use a window. Three clicks take what's connected, but
  on a circle's centre that means a single vertex, so click away from the centre.
- **A window takes only what lies wholly inside it,** and that includes other objects. Faces that cross its edge are left
  out. Where it would catch other objects, use `d.select`.
- **After Push/Pull** only part of the solid may stay selected. Reselect everything before Make Object, and check that the
  root has no triangles left.
- **After a handle drag, the handle is selected,** not the object. Click the object before Move Copy.

**Tools**

- **Rotate Copy takes three picks:** centre, reference, final angle. Typing `30 x11` at the third gives eleven more at 30°.
- **Copy repeats:** `xN` adds N copies, `/N` divides the span, and `2m x12` combines them. See
  [numeric-tool-input.md](../numeric-tool-input.md).
- **Add Hole:** click its first corner on clear wall. A first click at the base can land on the floor slab instead.
- **Door and Window** need a wall or slab under the first click to stand upright. Snap to an opening's corners, top corner first.
- **Wall faces:** with the template's offset 0.5, the chain line is the wall's outer face. A wall drawn in +X has that
  face looking to −Z, and one drawn in −X looks to +Z, so draw walls so the outer face looks at the camera.
- **Material editors are modal.** Save with `material.save`, then `d.close_modal()`. The ribbon is blocked until it closes.
- **Materials belong to each instance,** so they don't reach copies. Definitions are shared, and instance values are not.
- **The script editor preview** opens close up. Zoom with `d.wheel`, and check it on a frame, because too many clicks leave it blank.
- **Esc over MCP clears the 3D cursor** until the next hover, which `d.aim` does anyway.

**Hotspots**

- **Hotspots are defined inside an open object** and operated on a closed, selected instance. Handles only respond when
  that one instance is selected.
- **Coordinates are local.** Geometry and hotspot records use the object's frame, and its origin may be off the grid. Add
  `transform.translation` to get world positions, and compare hotspots with each other.
- **Attach goes to the older hotspot** when two are selected, whatever the order. Place the driving handle first.
- **Attach replaces** what the hotspot held. Select everything in one go.
- **A Stretch handle moves everything attached by one offset,** so it sizes one direction, and its marker must sit on the
  grid so a drag doesn't skew the object. Attach the corners of any face crossing the window, or that face stays behind.
- **Multiply handles need a reference,** and a marker at k steps gives k+1 elements, except at its starting spot, which gives one.
- **Evaluation runs after a drag,** so poll until it settles. See `settled_max_x` in `06-curtain-wall.py`.

**Files and projects**

- **An opened project's root isn't at `[]`.** Use `d.root()["path"]`. Parts of a closed object report as `occurrence` targets.
- **Recipes that continue a project** depend on an earlier recipe's saved `.octd`, so record them in order.

## Narration

- **Length:** a line holds at most two rows of 58 characters, about 115 in total, or the build stops. Most lines are 40 to
  80 characters.
- **Content:** say what to do, and why if it isn't obvious. Don't narrate what's already on screen.
- **Truth:** back each claim with `d.require`. Checks have caught lines that sounded right and were false, such as a material
  reaching every instance.
- **Pronunciation:** add an entry to `SPOKEN` in `scripts/video/subtitles.py` when the written form reads badly, such as
  `Push/Pull`. Changing an entry changes what's said, so those clips regenerate.
- **Stretched words are risky.** The owner asked for some ("niiiice", "wooow"), with restraint. This model can run away on
  sounds, and one clip came out as a long repeated "oh oh oh". Prefer plain words, and use a stretched one rarely. Tell the
  owner which lines to listen to.
- **Runaway clips:** `voice.py` retries any clip under 6 characters per second, then stops. If it keeps failing, reword the line.

You probably can't hear audio. If not, say so plainly, report what you measured, and ask the owner to listen. Don't claim
the narration sounds right.

## Review before you call it done

**Passing checks are not enough.** Checks missed glass that didn't fill its frame, a bookcase out of the frame, a blank
preview and a misleading keycap. All of those showed up only in frames. Look at the video.

```sh
V=docs/tutorials/videos/NN-name

# A contact sheet of the whole video. Open the image and look at it.
ffmpeg -y -loglevel error -i $V.mp4 -sn -vf "fps=1/9,scale=480:-1,tile=5x5:padding=3" -frames:v 1 /tmp/sheet.png

# Full-size frames at the moments that matter. Pick times from $V.cues.json.
ffmpeg -y -loglevel error -ss 61 -i $V.mp4 -sn -frames:v 1 /tmp/frame.png

# One video, one sound and one subtitle track, at about -16 LUFS.
ffprobe -v error -show_entries stream=codec_name:format=duration -of csv=p=0 $V.mp4
ffmpeg -hide_banner -i $V.mp4 -map 0:a -af ebur128 -f null - 2>&1 | grep ' I:' | tail -1

# mpv shows one subtitle set; the marked track is the active one.
mpv --no-config --no-audio --vo=null --frames=1 --sub-auto=exact $V.mp4 | grep Subs

# Speech rate of each line: flag anything far below the median of about 11 characters per second.
python3 - <<'PY'
import json, sys
sys.path.insert(0, "scripts/video")
import subtitles
voices = json.load(open("docs/tutorials/voice/index.json"))
for cue in json.load(open("docs/tutorials/videos/NN-name.cues.json"))["cues"]:
    clip = voices[cue["voice"]]
    print(f'{cue["start"]:6.1f}s {len(clip["spoken"]) / clip["seconds"]:5.1f} cps  {cue["text"][:60]}')
PY

# Nothing personal in what gets published.
grep -l "$(whoami)" docs/tutorials/projects/NN-name.octd $V.cues.json
```

Look for:

- The model stays in frame and isn't covered by a panel.
- The 3D cursor and snap label show at each pick, and helpers and selection windows are visible.
- Geometry looks right after every operation: gaps, parts left behind, frames away from walls.
- Keycaps teach the right thing, and the title bar shows no home path.
- Previews land before each click, and the ending has its card.
- The final frame shows the finished model.

Software rendering makes previews trail the cursor by a few tenths of a second, which is why `d.aim` pauses. The arrow also
stays an arrow during tools, because Xvfb has no full cursor theme. Both are known and accepted.

## When the app is wrong

Tutorials regularly find real bugs: a 3D cursor too small to see in centimetre projects, doors lying flat, overlapping
copies. If a natural action fails for a user, **fix the app rather than working around it in the video**:

1. Prove it with a failing test.
2. Make the smallest fix, and check other callers of what you changed.
3. Add the behaviour to the spec in `docs/rewrite/spec/`, and to [docs/mcp.md](../mcp.md) if MCP changed.
4. Rebuild, and dry-run the other recipes that use it.

Don't weaken a check, or relax a rule above, to get a build through. If something is a design decision rather than a bug,
describe it to the owner and let them choose.

## Finishing

The owner's standing rule: **finished work is always committed, tagged, pushed and released, without asking.**

1. **End cards:** make the previous tutorial's card point to the new one, then re-record and reassemble that video. Its
   clips are cached.
2. **Docs:** add the video to the table in [README.md](README.md). Add a task to `docs/rewrite/tasks.json`, a scenario to
   `docs/rewrite/acceptance.md`, a handoff in `docs/rewrite/handoffs/` with exact results and what wasn't verified, and an
   entry at the top of `DASHBOARD.md`.
3. **Inventory:** run `python3 scripts/export_tool_inventory.py`, because `docs/tools/index.html` embeds the version.
4. **Gate:** all of these must pass, and stop if any fails. Don't chain the commit after them with `;`.

   ```sh
   cargo fmt --all --check
   cargo test --workspace --locked
   cargo clippy --workspace --all-targets --locked -- -D warnings
   python3 docs/rewrite/scripts/verify_plan.py
   python3 scripts/export_tool_inventory.py --check
   python3 -m unittest discover -s scripts/video -p 'test_*.py'
   ```

   If you changed the app, run the relevant `scripts/verify_*.py --binary target/debug/meshcraft` too. Their captures rewrite
   evidence, so `git restore docs/rewrite/references` afterwards unless the evidence should change.
5. **Commit:** check `git status` first. Files someone else has staged go in the commit too, by the owner's rule, but scan
   them for secrets first and report anything large. Leave `CLAUDE.md` and `tasks/` alone unless they're staged. End the
   message with the attribution line your session specifies.
6. **Release:** bump the minor version for a new tutorial or feature, and the patch for a fix. Tags have no `v`, must equal
   the version in `apps/desktop/Cargo.toml`, and are never moved.

   ```sh
   git tag -a 0.55.0 -m "Meshcraft 0.55.0: what changed"
   git push origin work
   git push origin 0.55.0
   gh workflow run distribution.yaml --ref 0.55.0
   ```

7. **Report:** give the run link, then check on it and report the result. A failed job skips publishing. If the cause is outside
   the code, such as an HTTP 504 on a download, release the next patch version.

Finally, tell the owner where the video is, what you checked, what you couldn't check, which lines to listen to, and any app
limits you found.

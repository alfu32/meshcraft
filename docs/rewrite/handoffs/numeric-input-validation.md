# Buffered numeric tool input — 0.29.0

Scope: shared point workflows and T15/T21 copy transforms. The compact buffer is
transient and uses the same point handlers/document transactions as mouse picks.
The native GPUI shell, browser keyboard adapter and MCP text/backspace/finish path
share its parser. Read [syntax and examples](../../numeric-tool-input.md).

`cargo test --workspace --quiet` passed **670 regular tests** (283 core, 340
desktop, five CLI, 42 IO); five manual renderer probes were ignored. New checks
exercise physical unit conversion without rescaling internal values, relative
plane/world coordinates, invalid input/counts, retained/repaired buffers, Circle
radius input, source-preserving Move Copy multiplication/division, signed rotation
angle divisions, Scale Copy interpolation and Undo. The earlier test restricting
copy mode to movement/rotation was updated for the newly supported scale tools.

`cargo clippy --workspace --all-targets -- -D warnings`, desktop build, browser
library compilation and formatting pass. The browser check retains three existing
warnings about native-only walkthrough/clipboard methods. No other-platform GUI
qualification is implied.

Copy counts mean N new copies plus the source. Multiplication composes the chosen
increment; division includes the chosen endpoint and interpolates scale from
identity. The preview is bounded to 64 copies and shares source wireframe edges.
Counts are bounded by the existing 10,000-copy array limit. The existing atomic
array operation rejects singular/nonfinite transforms without publishing a partial
result. Repetitions are entered during the destination preview, before its commit.

## Native qualification

An isolated Xvfb/software-GPU run at 1500×1000 used MCP on its own port 18766
and sent real X11 keyboard events into the GPUI viewport. It typed `30cmm`,
removed the final character with Backspace, ignored Left/Delete, and committed
`30cm` as one internal unit with cm/30 model settings. A following relative
`2,3,4` advanced the chain to the expected endpoint, within floating tolerance.

Move Copy `/4` over an eight-unit span created offsets 2/4/6/8. `x3` repeated a
two-unit step at 2/4/6, and Undo restored the source. `x0` left both the revision
and editable buffer intact. The native Rotate Copy alias uses Rotate 2: after
its center/reference picks, `90deg /3` created 30°/60°/90° copies. Live MCP
text/finish also placed a root hotspot at relative XYZ 1/2/3 while the ordinary
selection tool remained the shell's active label. The user's running application
was not stopped and its model files were not used as test inputs.

Evidence: [unit entry](../references/numeric-input/numeric-units-preview.png),
[division wireframe](../references/numeric-input/numeric-divide-preview.png),
[rotation input](../references/numeric-input/numeric-rotate-preview.png),
[rotation result](../references/numeric-input/numeric-rotate-result.png),
[native results](../references/numeric-input/numeric-native-validation.json).

Browser compilation and shared/headless keyboard checks pass; an interactive
browser session, Windows/macOS and a physical GPU were not exercised in this pass.
All eight release-helper tests and the plan/local-link verifier pass.

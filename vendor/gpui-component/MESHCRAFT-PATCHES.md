# Meshcraft component patch

Vendored from the published `gpui-component` 0.5.1 crate. Original source and
Apache-2.0 license are preserved. Optional language features remain disabled;
Meshcraft registers only its JavaScript grammar separately.

Local change: `src/highlighter/highlighter.rs` uses `RopeSlice::chunks()` for
Tree-sitter node text. The upstream cursor advanced before yielding, omitting
its first chunk, and did not clip yielded chunks to node boundaries. As a
result, name predicates failed for short scripts and could read unrelated text
in longer scripts. Meshcraft's editor tests cover actual highlighted styles,
including single-chunk scripts and symbols after multiple rope chunks.

Removed deprecated `std::usize` module imports in the highlighter and diagnostics.
Existing `usize::MIN` / `usize::MAX` expressions now resolve to the primitive
type associated constants, preserving their values without deprecation warnings.


The color picker has local HSL (default), RGB and Swatches modes. HSL and RGB
provide a two-dimensional swatch, normalized channel sliders and numeric values;
all modes have a checkerboard-composited opacity bar and RGBA hex entry. Swatch
selection preserves opacity and stays open for refinement. Invalid channels do
not replace the current value. State synchronization does not emit selection
changes. The extension lives in `src/color_picker_channels.rs` and uses the
existing component slider/input subscriptions without a network service.


Input refresh fixes: `input/element.rs` schedules deferred focus cleanup only for
the input that lost focus, and rechecks current focus before clearing it. Painting
an unfocused input no longer continually queues another frame. `input/blink_cursor.rs`
tracks whether blinking is enabled by focus; programmatic text replacement cannot
start an unfocused cursor timer. Blur cancels pending blink/pause work. These fixes
prevent synchronized property panels from leaving dozens of recurring UI timers.

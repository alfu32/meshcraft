# Selected segment feedback — 0.45.2

The [requested rendering](requested.png) is retained beside the [actual live app
capture](actual.png). The real SDK [recipe](recipe.js) creates a closed contour
with a sampled curved corner and a neutral surface; [model.octd](model.octd) is
its saved MCP-created project. Enter its prototype and select its 16 segments,
leaving the faces unselected. [status.json](status.json) records that exact
selection and edit context.

Selection renders red dashed lines, small red endpoints, green midpoints and
arrowheads pointing in stored segment direction. One blue selection box remains;
its pass precedes selected linework so a coplanar box cannot obscure red edges.
The face material remains unchanged. Whole-object/face selection recolors lines
without adding thousands of primitive point markers.

The retained line shader projects the glyphs at fixed pixel sizes. Four glyph passes
share a cached endpoint buffer for endpoint A, endpoint B, midpoint and arrow;
there are no per-segment CPU arrow projections or sphere draws. Mesh uploads
occur on selection/content changes, not camera navigation.

Native OpenGL checks (private Xvfb/Mesa):

```sh
xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 cargo test -p meshcraft-desktop --lib --locked gpu_lines_match_screen_projection_and_clip_at_the_near_plane -- --ignored --test-threads=1
xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 cargo test -p meshcraft-desktop --lib --locked gpu_buffers_survive_navigation_selection_and_transform_only_edits -- --ignored --test-threads=1
```

Both passed: perspective/orthographic projection, width and near-plane clipping,
red/green pixels, primitive selection, shared instances, retained GPU resources,
and unchanged geometry/feedback upload counts during navigation. Exact outputs
are [line checks](gpu-lines.log) and [instance checks](gpu-sharing.log). This is
correctness/resource-reuse evidence, not a large-model frame-rate benchmark.

## Readability follow-up — 0.45.3

[Updated native capture](clear-markers.png): endpoint disks now have a larger
colored center, dark keyline and white halo. Direction arrows use an outlined
shaft/head instead of a small filled triangle. Four cached GPU passes are
retained; endpoints draw last so nearby arrows cannot obscure them. Arrows are
omitted below 32 projected pixels where they cannot fit clearly; point markers
remain and zoom reveals the arrow. The original 0.45.2 capture stays above for
comparison.

The [updated GPU check](gpu-lines-0.45.3.log) verifies a visible red area at both
endpoints, visible shaft/head pixels and reversal of the rendered arrow when
endpoint order reverses, as well as cached resources across camera navigation.

# ADR-05: Geometry precision and primitive semantics

- Status: Accepted for the numeric and primitive boundary; ownership/layout contracts remain gated by T46.
- Date: 2026-10-18

## Decision

Use `f64` for model-space calculations and retain line segments and triangles as the initial authored geometry primitives. Convert to `f32` only at GPU upload boundaries. Keep tolerance policies named by operation and purpose; do not collapse legacy line, face, hotspot-key and snapping tolerances into one epsilon.

The stack spike uses `i_overlay` for polygon boolean and contour operations and `i_triangle` for triangulation. Those crates are experimental dependencies only until their fixture results and the T46 ownership decision are reviewed together. T46 still decides canonical vertex identity, shared definitions/layouts, authored occurrence edits and cache ownership before stable document or geometry-store APIs are introduced.

## Rationale

The legacy application stores geometry in `f32`, while its operations use distinct tolerance values and preserve segment/triangle editing semantics. A double-precision model boundary reduces accumulation error without implying that coordinate values are meters or that near-coincident authored vertices are identical.

## Consequences

- GPU-facing structures may use narrower values, but must derive them from stable model-space data.
- Import, rendering, predicates, snapping and serialization must each state their numeric conversion and tolerance behavior.
- This ADR does not settle normalized storage or authorize position-based vertex merging.

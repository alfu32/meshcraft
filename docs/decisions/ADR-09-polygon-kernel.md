# ADR-09: Polygon clipping and triangulation

- Status: Accepted for the initial Rust implementation, pending task-spike execution evidence.
- Date: 2026-10-18

## Decision

Adopt pure-Rust `i_overlay = 9.1.0` for polygon union/intersection/difference, contour extraction and self-intersection handling. Use `i_triangle = 0.50.1` for triangulating concave contours and polygons with holes. Keep exact versions in the stack experiment and workspace lockfile. Both crates accept floating-point coordinates and provide deterministic integer-backed APIs; operation-specific quantization and coordinate-range checks remain explicit at the core boundary.

The overlay crate is the polygonization candidate for self-intersecting linework. Its output is a set of contours with the outer boundary and holes grouped by shape. Do not infer a closed-solid boolean from these 2D results; 3D mesh booleans remain a separate task and acceptance gate.

## Consequences

- The initial geometry stack remains available to browser/WASM builds without a native geometry runtime.
- Overlay output must be mapped back to source identities by the caller; the libraries do not define Meshcraft document identity or edit transactions.
- Near-collinear and scale-extreme input behavior is constrained by the recorded fixture runs and must be expanded by T13/T22 before release claims.

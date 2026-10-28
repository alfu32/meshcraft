# Live resource status — U04 / 0.43.0

Implemented process RAM (resident bytes / physical RAM), normalized process CPU,
and driver-dependent VRAM bytes/percentage in the native status bar. Metric cells,
numeric fields and viewport-frame fields have fixed widths. The status message
keeps its remaining space and full-text tooltip, without moving when values change.

OS sampling runs on a dedicated worker at 1 Hz. Only the HUD view is invalidated.
Process VRAM is preferred (Linux DRM / Windows DXGI); device-only fallback is
explicitly labeled **GPU total**. Missing/invalid/shared/software GPU counters
show **N/A**. Percentages use physical RAM, whole-machine CPU capacity, or the
reported dedicated VRAM capacity. No model units are involved. See the
[status requirements and counter references](../spec/interface.md#u04-preferences-status-and-feedback).

## Verification

- `cargo test -p meshcraft-desktop --lib resource_stats -- --nocapture`:
  6 passed, 1 separate GPU check ignored by default. Covers live process RSS,
  percentage normalization, exact bytes, stable field length, GPU scope fallback,
  GPU counter units/validity/software-renderer detection, DRM resident preference,
  missing-counter handling and duplicate-client suppression.
- `xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 target/debug/deps/meshcraft_desktop-0e73eb0cf140647c gpu_telemetry_supported_or_unavailable_without_gl_errors --ignored --nocapture`:
  passed. Actual OpenGL extension discovery/fallback leaves no GL errors.
- `cargo build -p meshcraft-desktop --bin meshcraft`: passed, 0.43.0.
- `python3 scripts/verify_resource_status.py`: passed using a private 1280×900
  desktop, isolated config, real MCP requests and unmodified native captures.
  Resident RAM/CPU readings were visible and updated. VRAM correctly showed N/A
  for Mesa software rendering. An initial software-driver pseudo-VRAM value was
  identified during testing and excluded before final verification.
- Viewport frame sequence stayed **1 → 1** and document revision **0 → 0** across
  3.2 idle seconds while resource updates continued. ImageMagick checks found
  visible RAM/CPU/VRAM/Frame labels at fixed x=413/659/763/1051 with **zero changed
  anchor pixels** between captures. Sampling did not force viewport redraws.
- `python3 scripts/export_tool_inventory.py --check --binary target/debug/meshcraft`:
  289 entries, no new modeling commands and no lost review assignments.
- `cargo fmt --all -- --check`, `git diff --check`, and
  `python3 docs/rewrite/scripts/verify_plan.py`: passed.

[Native status bar capture](../references/resource-status/process-status.png).
[Idle/anchor observations](../references/resource-status/idle.json).

The positive hardware VRAM paths and Windows/macOS runtime sampling were not
executed on this Linux software-GL desktop. Driver support determines availability;
the UI does not invent a zero or attribute device-wide usage to Meshcraft.

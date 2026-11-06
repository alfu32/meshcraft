# Feature backlog

## #27 — Efficient shading for growing scenes

Source: [GitHub issue #27](https://github.com/alfu32/meshcraft/issues/27).
Status: **Planned**; related renderer integration work is tracked by T60.

Investigate whether raylib manages shadow-camera coverage as the authored model
grows. Record the actual division of responsibility between raylib and Meshcraft
before choosing an implementation.

The requested behavior is full-scene shadow coverage. If Meshcraft must manage
it, fit the directional light's frustum to the effective scene bounds and assess
spatially partitioned shadow maps for larger models. Objects spanning partitions
must participate in each relevant shadow pass. Treat the synthetic ground's
special representation explicitly.

The acceptance work must measure coverage and quality as scene bounds grow,
including transformed instances and objects spanning partitions. Existing GPU
instancing, selection optimizations and the T78 ground-depth fix do not establish
this feature's completion. Keep the issue open until its investigation and
resulting implementation are resolved.

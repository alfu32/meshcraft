# T00 Baseline Results

## Source and fixture evidence

The reference checkout is pinned to `acd84f4c67509e10363c33850501eb8f4fc3ffa7` and was clean when captured on 2026-10-18. `fixtures/legacy/manifest.json` records all 60 indexed models with byte size, SHA-256, sparse root keys, payload families, and sample/opt-in status. Small selected fixtures are copied under `fixtures/legacy/samples/`; larger compressed models remain external references keyed by hash.

The historical `rootGroup` example was recovered with `git show` from `cf5f3c64819d246bbfb77c066a24fd8e63393459`. The inspected pre-scene-graph commit (`2438532a507cc9931eb734a5e5628d5ff34779d2`) has source code for flat geometry loading but no saved model fixture. The prototype commit (`c85b8b9fa1a4e5d2989bdacfdbbb15aeeaf683af`) likewise has a loader path but no prototype-only document. Those shapes remain explicit evidence gaps; no synthetic fixture is labeled historical.

## Runtime and performance baseline

The legacy README documents `./gradlew build` and `./gradlew :lwjgl3:run`; desktop Gradle configuration targets Java 21. The capture host is Ubuntu 25.10 x86_64 with an Intel Core i7-6700HQ, 8 logical CPUs, Java 21, and an active Wayland/X11 display. On 2026-10-18, the pinned clean checkout at `acd84f4c67509e10363c33850501eb8f4fc3ffa7` launched with `./gradlew :lwjgl3:run --args='edit --file /tmp/meshcraft-pinned-baseline.octd --size 1280x720'`, Gradle 9.2.1 and Temurin Java 21.0.9. The 5.7.2 window opened and rendered a disposable copy of `test-shortcuts.0.octd`; this is a manual startup/visual check, not a parity or performance result. The run logged a libGDX version warning (runtime 1.14.0 versus expected 1.13.5) and could not start its MCP server because the installed 5.7.1 editor already held port 8765. The disposable copy's bytes changed during the app process; no canonical comparison was made, so it is not treated as a baseline output. The checked-in fixture stayed at SHA-256 `e82fda561dc694edcfeec87e3dd7b059ef65bbf3d4ac4073ee817c8612cde131`. Gradle temporarily generated `BuildFlags.WEB_BUILD = false`; its original tracked value was restored and the legacy checkout is clean.

No timing baseline has been measured. Future measurements must record OS, CPU, GPU/driver, memory, build revision, fixture hash/size, and cold/warm state. Measure cold and warm load/save, perspective and orthographic picking percentiles, simple edit latency, frame time, preview generation, regeneration, and undo/history memory. Treat the documented 60 FPS and 50 ms goals as targets until measured.

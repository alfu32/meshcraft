# T00 Baseline Results

## Source and fixture evidence

The reference checkout is pinned to `acd84f4c67509e10363c33850501eb8f4fc3ffa7` and was clean when captured on 2026-10-18. `fixtures/legacy/manifest.json` records all 60 indexed models with byte size, SHA-256, sparse root keys, payload families, and sample/opt-in status. Small selected fixtures are copied under `fixtures/legacy/samples/`; larger compressed models remain external references keyed by hash.

The historical `rootGroup` example was recovered with `git show` from `cf5f3c64819d246bbfb77c066a24fd8e63393459`. The inspected pre-scene-graph commit (`2438532a507cc9931eb734a5e5628d5ff34779d2`) has source code for flat geometry loading but no saved model fixture. The prototype commit (`c85b8b9fa1a4e5d2989bdacfdbbb15aeeaf683af`) likewise has a loader path but no prototype-only document. Those shapes remain explicit evidence gaps; no synthetic fixture is labeled historical.

## Runtime and performance baseline

The legacy README documents `./gradlew build` and `./gradlew :lwjgl3:run`; desktop Gradle configuration targets Java 21. The capture host is Ubuntu 25.10 x86_64 with an Intel Core i7-6700HQ, 8 logical CPUs, Java 21, and an active Wayland/X11 display. GPU model, driver, and application build were not recorded. No live editor session or benchmark was run in this capture; all behavior evidence here is static. This is pending runtime evidence, not a finding that the legacy GUI cannot launch.

Future measurements must record OS, CPU, GPU/driver, memory, build revision, fixture hash/size, and cold/warm state. Measure cold and warm load/save, perspective and orthographic picking percentiles, simple edit latency, frame time, preview generation, regeneration, and undo/history memory. Treat the documented 60 FPS and 50 ms goals as targets until measured.

# Meshcraft: k3d / Octodraw Rust rewrite

The rewrite began as a source-backed specification and implementation handoff, prepared from `/home/alfu64/Development/k3d` at commit `acd84f4c67509e10363c33850501eb8f4fc3ffa7` (tag `5.7.2`). The first Rust model now supports interactive line, rectangle, cuboid, copy, object, and array workflows; full modeling, persistence, undo, and compatibility remain unimplemented.

Start with [the rewrite guide](docs/rewrite/README.md). It links the feature specification, compatibility requirements, architecture proposal, history analysis, acceptance scenarios, and dependency-ordered agent tasks.

Desktop is the first delivery target. It includes a conventional menu interface alongside the button-oriented workspace. Browser support and JavaScriptCore plugins with a versioned host context follow desktop qualification. The object model has an early normalization gate; the current Rust model is a prototype pending that gate. Android, Groovy/JVM plugin execution, and CLI/TUI developer tools remain deferred.

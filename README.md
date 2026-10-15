# Meshcraft: k3d / Octodraw Rust rewrite

The current deliverable is a source-backed specification and implementation handoff, prepared from `/home/alfu64/Development/k3d` at commit `acd84f4c67509e10363c33850501eb8f4fc3ffa7` (tag `5.7.2`). Implementation has not started.

Start with [the rewrite guide](docs/rewrite/README.md). It links the feature specification, compatibility requirements, architecture proposal, history analysis, acceptance scenarios, and dependency-ordered agent tasks.

Desktop is the first delivery target. It includes a conventional menu interface alongside the button-oriented workspace. Browser support and JavaScriptCore plugins with a versioned host context follow desktop qualification. The object model has an early normalization gate. Android, Groovy/JVM plugin execution, and CLI/TUI developer tools remain deferred.

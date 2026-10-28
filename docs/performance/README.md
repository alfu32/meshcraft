# Tool performance suite

The suite drives the **actual Meshcraft desktop binary** through MCP and native
mouse events in a private Xvfb display. It runs the same command registry,
world-point input, geometry feedback, transactions, host jobs and GPU renderer
used by the application. It does not build an alternative modeling implementation.

[Latest release report](https://github.com/alfu32/meshcraft/releases/latest/download/performance.md) ·
[Latest CSV](https://github.com/alfu32/meshcraft/releases/latest/download/performance.csv) ·
[Latest raw JSON](https://github.com/alfu32/meshcraft/releases/latest/download/performance.json)

[Latest diagnostic bundle](https://github.com/alfu32/meshcraft/releases/latest/download/performance-diagnostics.zip)

These assets are added by **Stress test after Release**. A newly published release
has no performance assets until its stress run finishes; the download URL does
not silently substitute another version's results. Previous releases retain their
own reports. The [tool inventory](../tools/tools.csv) is the coverage authority.

[Local 0.40.0 qualification](local-0.40.0/performance.md) includes the complete
four-tier ledger and raw data from a supplied development binary, with one sample
per case. It is explicitly separate from released-binary CI measurements. Its
provenance identifies the frozen executable used by all four local tier workers.

[Object baking qualification, 0.42.0](object-baking-0.42.0/performance.md)
records the two new commands on all four tiers: six completed operations and two
heavy-tier failures at the existing Undo snapshot budget. This filtered local run
is separate from the full-suite release reports.

## Coverage and model weights

Every inventory entry gets a recipe at **small, medium, high and heavy** sizes.
Registered construction tools accept all required picks, generate a live preview,
and commit geometry; transformations and arrays operate on valid selected sources.
File operations use actual temporary files and wait for asynchronous completion.
Other commands exercise their intended session/editor/read action. Deferred
commands have explicit `unsupported` rows with no successful timing values.
Operation failures and performance values are report data and never fail a
completed measurement run. Missing recipes, unknown IDs and incomplete execution
remain harness errors. CI still publishes a detailed diagnostic report for those
errors, clearly marked incomplete. A failed tool is never omitted or assigned a
successful zero time.

| Tier | Unique quads | Stored background vertices | Segments | Triangles | Background primitives | Extra fragment selection |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| small | 128 | 1,792 | 512 | 256 | 768 | 24 |
| medium | 2,048 | 28,672 | 8,192 | 4,096 | 12,288 | 192 |
| high | 16,384 | 229,376 | 65,536 | 32,768 | 98,304 | 1,536 |
| heavy | 65,536 | 917,504 | 262,144 | 131,072 | 393,216 | 6,144 |

The background is real loose geometry, not thousands of instances of one mesh.
Legacy import retains independent endpoint IDs for lines and triangles: each quad
has four unique positions but fourteen stored vertices. The runner verifies the
live background's vertex, segment and triangle counts after loading each tier.
Fixture hashes and byte sizes appear in JSON. Additional workpieces include two
shared cube instances, profiles, contour/path objects and a fragment mesh.

The standard recipes keep operation inputs small while the resident drawing grows.
Supplemental **fragment** variants of Select, Move and Rotate scale the selected
edge/triangle count as shown above, operating on primitive IDs inside prototype
edit mode. They expose large-selection feedback/commit costs separately from
whole-instance transforms. They are benchmark variants, not invented application
commands or changes to the menu-review CSV.

## Running locally

Use a Linux desktop build with `--mcp-port` support (0.40.0 onward), Xvfb, xdotool,
Mesa Vulkan/GL drivers, `glxinfo` and Python 3.10 or newer. No third-party Python
packages are required. The runner creates its own display, configuration, cache,
port, model files and processes; it does not drive an existing user window.

```sh
cargo build --release -p meshcraft-desktop --locked
python3 scripts/stress/run.py --binary target/release/meshcraft \
  --output target/stress-results --samples 3
```

Local native executables are copied into the output directory before discovery,
so concurrent Cargo builds cannot change the binary between process restarts.
For a release archive, use `download.py` to verify and extract it, then supply its
top-level `meshcraft` launcher with `--artifact provenance.json` and the matching
`--source-revision`. The launcher sets the bundled dependency paths and retains
the verified package layout. CI uses this path automatically.
Software rendering is the reproducible default. `--hardware` requests available
hardware GL but still uses a private display; inspect `gpu.txt` to confirm the
actual renderer rather than assuming the flag guarantees acceleration.

For development qualification:

```sh
python3 scripts/stress/run.py --binary target/debug/meshcraft \
  --output target/stress-smoke --tiers small --samples 1 \
  --only tool.line tool.builtin.move tool.builtin.rotate
python3 -m unittest discover -s scripts/stress -p 'test_*.py' -v
```

Filtered or single-tier reports are explicitly partial and cannot replace the
complete release report. CI merges four dedicated single-tier workers only after
checking binary/source identity, inventories, fixture coverage and sample counts.

## Reading the timings

Times are in milliseconds. Reports include raw samples, medians, nearest-rank p95
and maxima. CI collects three samples per successful case. A failed operation
stops that case's remaining repetitions, records its diagnostics and partial timing,
and the suite continues where possible. Setup failures have no operation latency.
One-sample development runs are qualification evidence, not a useful distribution.

- `start_ms`: command activation or the first state transition.
- `input_ms`: accepted intermediate points.
- `preview_ms`: final hover/phantom evaluation and its fresh viewport fence.
- `commit_ms`: final input/command and asynchronous job completion where applicable.
- `frame_ready_ms`: wait for the final current viewport frame. A small PNG capture
  acts as the fence; readback/encoding/transport contribute to this measurement.
- `total_ms`: sum of these timed client stages. Prerequisite setup, validation,
  reset/Undo and fixture loading are excluded. The raw workflow elapsed time is
  retained separately, including untimed recipe queries and prerequisites.
- `mcp_owner_thread_ms`: successful timed MCP calls' main-thread execution sum,
  excluding transport/queue delay and background-worker computation. Native mouse
  handlers are outside this sum. It is not total application CPU time.
- `render_frame_ms`: the final raylib offscreen frame's measured work, including
  readback. It is not a pure GPU timer or the GPUI compositor's frame time.

MCP overhead is real and disclosed; tiny operations can be dominated by transport
or readiness checks. Selection uses exact IDs, so this suite does **not** establish
screen hit-testing, marquee-selection or mouse-snap latency. UI actions are measured
as their intended state/editor operations, not claimed as mesh construction. A
capture fence confirms the 3D viewport, not completion of every GPUI widget paint.

Autosave is disabled only in the private benchmark process through
`MESHCRAFT_DISABLE_AUTOSAVE=1`. Initial configuration uses antialiasing Off and a
hidden permanent grid, with a 1280×900 window; display/UI recipes deliberately
change their relevant settings. GPU identity, CPU/platform, configuration, fixture weights, binary
SHA-256, source and all failures are recorded. Compare matching hardware/rendering
modes and workloads. Local development captures can include normal desktop load;
release CI has dedicated tier workers and does not compile during measurement.

## Running and publishing in CI

1. Run **Release** at an existing version tag and wait for its published binaries.
2. Run **Stress test** at that **same tag reference**. There is no redundant tag input.
3. Four Ubuntu workers independently download the tag's existing Linux x86_64
   release archive and checksum. They verify checksum, every packaged file,
   architecture, app version, tag and source SHA before launching it. No Cargo or
   app build step exists in this workflow. An absent release/binary fails.
4. Each worker runs its tier and publishes its Markdown summary plus a workflow
   artifact containing reports and diagnostics, including on failure.
5. The publication job always uploads `performance.md`, `performance.csv`,
   `performance.json` and `performance-diagnostics.zip` to the same existing
   release, irrespective of measured performance or tool success/failure.
   Complete validated runs retain all timing tables and failure details. Failed
   measurements do not make the benchmark job red.
6. If workers are missing, interrupted, corrupt or mismatched, publish an explicit
   **incomplete diagnostic report** with all available unvalidated observations,
   runner output and native logs. These replace the canonical report assets as
   the latest attempt, while making no claim of complete validated measurements.
   Strict source/binary/coverage checks still govern measurement tables. Missing
   binaries, setup and release-upload errors remain infrastructure failures.

The workflow currently qualifies Linux x86_64 software rendering. The six-target
**release** matrix remains unchanged. This is not Windows/macOS performance evidence.
Use the versioned report assets to compare releases; the README's latest-release
links stay stable. The workflow never creates/moves tags or creates a replacement
release when binaries are missing.

For example, after publishing the source tag, dispatch Release and wait for it to
finish before dispatching Stress test:

```sh
gh workflow run distribution.yaml --ref 0.40.1
# After the release succeeds and its binaries are published:
gh workflow run stress-test.yaml --ref 0.40.1
```

## Findings and interpretation

The frozen local ledger contains **907 passed, 129 failed and 124 unsupported**
cases. Small and high have no failed supported cases; medium has one editor
crash. Heavy has 121 Undo-budget failures, six MCP request expiries and one editor
crash. These are observed outcomes, not successful latency measurements. The
tracked reports retain partial timings, native crash logs, all 36 superseded
timing observations and the reasons for focused retiming. Three-sample small-tier
tool-cycle checks are included separately in JSON.

The heavy fixture currently exposes the desktop's default 256 MiB undo snapshot
budget: some commits need roughly 451 MB for the before/after states and are
rejected atomically. Those are genuine application failures; they remain failures
in the reports. Raising the limit solely for the benchmark would conceal the
limitation. The full ledger distinguishes these failures from missing recipes,
unsupported commands, setup problems and successful operation measurements.

The local qualification also caught a GPUI 0.2.2 image-atlas panic during a rapid
`script.new` editor open/close sequence at medium/heavy weight. The process recovery
path records the failed case, preserves the native log and continues on a fresh
private process. The observed panic is in `platform/blade/blade_atlas.rs:229`;
its underlying cause has not been established by this performance delivery.

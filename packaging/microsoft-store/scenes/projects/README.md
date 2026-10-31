# Project-scale modeling recipes

These recipes create the editable projects used by the ten Store screenshots.
Run them through `scripts/capture_store_projects.py`, which discovers Meshcraft's
MCP contract, creates materials and geometry, invokes native tools, saves the
projects and captures the real application. See the [view/project mapping](../README.md).

| Sources | Project |
| --- | --- |
| [instrument.js](instrument.js) | Controller chassis, ventilation, PCB, display, controls, connectors and service-cover assembly |
| [cabinet.js](cabinet.js), [kitchen.js](kitchen.js) | Shared API v2 cabinet prototype and fitted kitchen environment |
| [casework.py](casework.py) | Three cabinet configurations with books, doors and shelf storage; JavaScript editor preview |
| [residence.py](residence.py), [residence.js](residence.js) | Native courtyard-house architecture plus solar, furniture and site details |
| [rooftop.py](rooftop.py), [landscape.js](landscape.js) | Furnished roof terrace with five procedural material families |
| [park.py](park.py), [canopy.glsl](canopy.glsl), [landscape.js](landscape.js) | Pocket park and custom GPU branching program; shader and object editor views |
| [loft.py](loft.py), [loft.js](loft.js) | Native turning stair and furnished double-height interior |
| [plantroom.py](plantroom.py), [plantroom.js](plantroom.js) | Pump skid and native spatial Sweep of an I-section through two right-angle turns |
| [studio.py](studio.py), [studio.js](studio.js) | Home office, native dimension annotation and orthographic camera |
| [common.js](common.js) | Material lookup and bounded box, quad, rod and hollow-tube construction helpers |

Dimensions use internal model units. Instrument geometry uses one millimetre per
unit; the other projects use ten centimetres per unit. Material and procedural
parameters follow each program's actual schema. The capture driver's final
review step adjusts camera framing, grain, foliage and tree scale to those units.

The cabinet source declares a prototype plate-thickness parameter and instance
size, layout, opening, shelf, finish and hotspot parameters. Generated geometry
reads those values in `onAnimationFrame`. Repeated kitchen cupboards share the
same prototype but retain independent parameters. The canopy program changes
the GPU branch rule instead of constructing a CPU tree mesh.

Complex projects are assembled from meaningful bounded components, preserving
the application's script time and geometry limits. Walls, slabs, openings,
frames, stairs, dimensions and the spatial Sweep are native commands, rather
than geometry substitutes claiming to test those tools. These projects illustrate
modeling features; their capture times and scene counts are not performance
benchmarks. The models are concept studies rather than construction documents.

```sh
# Rebuild one project, without capturing the final Store slots:
python3 scripts/capture_store_projects.py --project plantroom --build-only
# Reopen the saved result and capture its final view:
python3 scripts/capture_store_projects.py --capture-only --shot 09-mechanical-assembly
```

The final `.octd` models are versioned under `captures/projects/`, together with
source recipes, screenshots and capture evidence outside `target`.

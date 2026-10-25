# Meshcraft performance — 0.40.0

Source revision / working-tree base: `7bdcf8e5b107242d7c665748c83c06b70f6230b8` · Binary SHA-256: `7dce78db69c44814adb8b61a0ca84342b6d2aaf8565336c28898f73fe62e7604`

Environment: Linux-6.17.0-41-generic-x86_64-with-glibc2.42 · See per-tier environments in JSON · See per-tier renderer identities in JSON
Provenance: frozen local working-tree development executable; source_revision records its working-tree base, not an exact release source claim · Build: local development executable, frozen before testing · Activity: four local private desktop processes measured concurrently; shared host CPU/GPU and normal desktop load
Samples per tool/tier: **1** · Started: 2026-10-25T11:06:37.566397+00:00

Coverage: **1160 cases** — 129 failed, 907 passed, 124 unsupported

## What is measured

**Local development qualification, not a released-binary speed baseline.** One sample per main case; development profile; Mesa software GL; four private desktop processes initially measured concurrently on the same host, with normal desktop background activity. The working-tree base revision alone does not reproduce the uncommitted application changes used by this binary. Binary SHA-256 identifies the executable actually tested.

All four tier workers used frozen copies of the same executable and verified native fixture counts. Focused recipe checks retimed all four tiers after separating prerequisite creation from operation timing, adding floating-preview fences, and completing procedural tree instance placement. All 36 superseded observations and their replacement reasons remain in JSON. Earlier preliminary measurements whose executable changed during capture are excluded entirely.

Release-binary CI measurements and release-asset publication have not run for 0.40.0: its published release binary does not yet exist. Run Release, then Stress test at the same tag. These tracked local reports are separate qualification evidence. No fixture weights or Undo limits were reduced to hide failures.

[Native crash diagnostic](diagnostics/medium-process-2.log). The underlying GPUI image-atlas cause has not been established by this delivery.

[Native crash diagnostic](diagnostics/heavy-process-2.log). The underlying GPUI image-atlas cause has not been established by this delivery.

Real desktop operations through MCP, including world-point input, phantom feedback, committed edits, asynchronous jobs and fresh GPU viewport readiness. Timings use a monotonic client clock and include localhost transport/main-thread scheduling. Selection uses exact entity IDs; these numbers do not measure screen hit-testing or mouse snapping. Session commands measure their intended state transition, not construction geometry. Deferred commands are explicitly unsupported, never successful zero-ms tools.

Setup, fixture loading, prerequisite selection, correctness queries, undo/reset and cleanup are excluded. `total` sums the timed operation stages and wait for a fresh viewport (untimed prerequisite/correctness calls are excluded); it includes the small PNG capture used as a readiness fence, not a GPUI window-paint acknowledgement or GPU-only timer. `commit` includes asynchronous completion where applicable. Initial/final point input is distinguished from preview. Autosave is disabled only in the private benchmark process. Fixed-size operation inputs run against increasingly large resident unique meshes; this is not a claim about transforming all background faces. Supplemental fragment probes for Select, Move and Rotate grow the selection to 24 / 192 / 1,536 / 6,144 primitives across the tiers, and operate on primitive IDs in prototype edit mode.

Hardware and software runs must be compared separately. These are observations, not portable throughput guarantees. No threshold is inferred from a different GPU or CI runner. All raw samples, steps and failure diagnostics are in performance.json; CSV contains every case including failures/unsupported entries.

## Model tiers

| Tier | Stored background vertices | Segments | Triangles | Rendered primitives |
| --- | ---: | ---: | ---: | ---: |
| small | 1,792 | 512 | 256 | 768 |
| medium | 28,672 | 8,192 | 4,096 | 12,288 |
| high | 229,376 | 65,536 | 32,768 | 98,304 |
| heavy | 917,504 | 262,144 | 131,072 | 393,216 |

Workpieces add two shared cube placements and small contour/profile/path objects. Fixture SHA-256 hashes, exact bytes and workload details are recorded in JSON.

## Slowest completed operations

| Tier | Tool / command | Total median (ms) | Total p95 (ms) | Preview median (ms) | Commit median (ms) | MCP owner median (ms) | Viewport frame median (ms) |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| high | Snapshot Document (`document.snapshot`, standard) | 13986.441 | 13986.441 | — | 13939.973 | 3571.817 | 618.427 |
| heavy | Open Project… (`file.open`, standard) | 12727.607 | 12727.607 | — | 9725.318 | 26.588 | 2462.145 |
| heavy | Save (`file.save`, standard) | 12616.350 | 12616.350 | — | 12229.535 | 524.970 | 291.065 |
| high | Volumetric Translate Multiple (`tool.builtin.volumetric_translate_multiple`, standard) | 12522.577 | 12522.577 | 292.770 | 11690.152 | 11592.174 | 430.845 |
| heavy | Save As… (`file.save_as`, standard) | 11991.499 | 11991.499 | — | 11329.598 | 479.641 | 182.612 |
| high | Array Selection (`object.array_selection`, standard) | 8054.649 | 8054.649 | — | 7795.020 | 7575.623 | 584.545 |
| heavy | Copy as Script (`edit.copy_as_script`, standard) | 7312.522 | 7312.522 | — | 6970.304 | 6638.076 | 321.416 |
| high | Import Scene as Prototype (`object.import_scene`, standard) | 6903.463 | 6903.463 | 1366.642 | 1777.319 | 2190.444 | 699.078 |
| high | Reference External Scene (`object.reference_scene`, standard) | 6644.730 | 6644.730 | 1136.982 | 1758.964 | 2152.768 | 690.274 |
| high | Mesh (`tool.builtin.mesh`, standard) | 6328.897 | 6328.897 | 1230.114 | 2172.063 | 3918.992 | 103.140 |
| high | Copy Multiple (`tool.builtin.copy_multiple`, standard) | 6253.665 | 6253.665 | 680.652 | 4271.470 | 3993.802 | 780.871 |
| high | Planar Translate Multiple (`tool.builtin.planar_translate_multiple`, standard) | 6199.886 | 6199.886 | 335.415 | 5138.101 | 5056.777 | 607.011 |
| high | Extrude Swipe 3D (`tool.builtin.extrude_swipe_3d`, standard) | 6132.964 | 6132.964 | 855.591 | 3225.814 | 2860.352 | 876.616 |
| high | Helical Array (`tool.builtin.helical_array`, standard) | 5972.523 | 5972.523 | 750.075 | 3333.785 | 3025.618 | 677.165 |
| heavy | Exit Object Edit (`selection.exit_context`, standard) | 5814.102 | 5814.102 | — | 2000.757 | 1598.099 | 2597.905 |
| high | Helical Array Even (`tool.builtin.helical_array_equalized`, standard) | 5524.433 | 5524.433 | 720.041 | 3126.921 | 2857.175 | 663.075 |
| heavy | Edit Prototype Script (`script.edit`, standard) | 5333.927 | 5333.927 | — | 330.494 | 24.276 | 3059.001 |
| high | Helicoidal Rotate Multiple (`tool.builtin.helicoidal_rotate_multiple`, standard) | 5057.354 | 5057.354 | 762.346 | 2500.056 | 2247.047 | 709.704 |
| high | Circle (`tool.builtin.circle`, standard) | 5023.849 | 5023.849 | 794.390 | 3279.337 | 3325.889 | 774.020 |
| high | Extrude Swipe (`tool.builtin.extrude_swipe`, standard) | 4959.311 | 4959.311 | 712.884 | 2520.929 | 2265.615 | 767.091 |
| high | Double Line (`tool.builtin.double_line`, standard) | 4862.435 | 4862.435 | 556.536 | 2816.786 | 2606.812 | 614.220 |
| high | Toggle Copy Mode (`tool.copy_mode`, standard) | 4589.856 | 4589.856 | 575.303 | 2591.186 | 2386.299 | 746.590 |
| high | Configure Procedural Object (`object.configure_procedural`, standard) | 4558.444 | 4558.444 | 876.634 | 3056.528 | 3172.957 | 1009.604 |
| high | Distance Input (`tool.distance`, standard) | 4530.201 | 4530.201 | 696.368 | 2129.703 | 1970.661 | 854.349 |
| high | Open Project… (`file.open`, standard) | 4337.484 | 4337.484 | — | 3324.149 | 25.393 | 681.845 |
| high | Construction Line (`tool.builtin.construction_line`, standard) | 4335.815 | 4335.815 | 767.290 | 2459.650 | 2142.801 | 654.167 |
| high | Wall (`tool.builtin.arch_wall`, standard) | 4260.831 | 4260.831 | 811.481 | 2298.757 | 1919.753 | 851.561 |
| high | Freeform Slab (`tool.arch_slab_freeform`, standard) | 4247.856 | 4247.856 | 764.114 | 1925.445 | 1645.248 | 733.857 |
| high | Line (`tool.builtin.line`, standard) | 4238.281 | 4238.281 | 845.102 | 2323.431 | 2011.717 | 717.842 |
| high | Save (`file.save`, standard) | 4164.545 | 4164.545 | — | 3933.078 | 123.221 | 79.214 |

## Complete tool coverage

Each row below represents an inventory command. Cells show **total median ms**, `unsupported`, or the failure state. Every listed tier must have a result; missing recipes and missing cells fail the runner.

| Command | small | medium | high | heavy |
| --- | ---: | ---: | ---: | ---: |
| `annotation.update` / standard | 400.693 | 436.594 | 545.689 | failed |
| `app.quit` / standard | 179.854 | 199.850 | 160.207 | 170.885 |
| `architecture.add_hole` / standard | 350.717 | 452.465 | 1891.357 | failed |
| `architecture.create_frame` / standard | 436.459 | 590.338 | 1798.092 | failed |
| `architecture.create_rectangular_slab` / standard | 373.286 | 559.639 | 1911.566 | failed |
| `architecture.create_slab` / standard | 405.572 | 653.282 | 1708.890 | failed |
| `architecture.create_stair` / standard | 367.054 | 496.462 | 1570.136 | failed |
| `architecture.create_wall` / standard | 302.351 | 485.818 | 1564.035 | failed |
| `architecture.move_control` / standard | 425.954 | 484.449 | 1529.896 | failed |
| `architecture.move_slab_vertex` / standard | 274.945 | 623.324 | 1431.904 | failed |
| `architecture.remove_hole` / standard | 387.692 | 459.141 | 1530.898 | failed |
| `architecture.settings` / standard | 339.514 | 503.735 | 2586.708 | failed |
| `architecture.stair_contour` / standard | 514.163 | 570.843 | 524.329 | 790.050 |
| `architecture.stair_path` / standard | 487.290 | 665.165 | 2444.646 | failed |
| `architecture.update` / standard | 276.426 | 507.020 | 1630.289 | failed |
| `document.snapshot` / standard | 482.371 | 2016.271 | 13986.441 | failed |
| `edit.clear_selection` / standard | 498.153 | 434.731 | 692.882 | 683.675 |
| `edit.copy_as_script` / standard | 516.364 | 671.926 | 2023.476 | 7312.522 |
| `edit.copy_selection` / standard | 489.203 | 547.625 | 796.638 | 1967.788 |
| `edit.cut_rect_hole` / standard | unsupported | unsupported | unsupported | unsupported |
| `edit.cut_selection` / standard | 482.535 | 779.760 | 2712.995 | failed |
| `edit.delete_selection` / standard | 484.537 | 749.229 | 1969.746 | failed |
| `edit.duplicate_selection` / standard | 509.472 | 811.492 | 2589.251 | failed |
| `edit.group` / standard | 470.281 | 725.415 | 2548.532 | failed |
| `edit.new_architecture_group` / standard | unsupported | unsupported | unsupported | unsupported |
| `edit.new_voxel_group` / standard | unsupported | unsupported | unsupported | unsupported |
| `edit.paste` / standard | 1368.504 | 1584.755 | 4154.320 | failed |
| `edit.paste_in_place` / standard | 509.716 | 844.571 | 2538.720 | failed |
| `edit.redo` / standard | 560.120 | 590.607 | 1109.184 | failed |
| `edit.reset_scene_for_capture` / standard | 1685.046 | 931.082 | 1439.322 | 2742.994 |
| `edit.undo` / standard | 555.953 | 567.137 | 1119.209 | failed |
| `edit.ungroup` / standard | 482.650 | 684.652 | 2463.223 | failed |
| `edit.voxelize_faces` / standard | unsupported | unsupported | unsupported | unsupported |
| `export.ifc_model` / standard | 696.273 | 1015.613 | 1552.477 | 2879.868 |
| `export.mesh` / standard | 731.037 | 717.052 | 1184.989 | 3022.279 |
| `export.screenshot` / standard | unsupported | unsupported | unsupported | unsupported |
| `export.svg_view` / standard | unsupported | unsupported | unsupported | unsupported |
| `file.cancel_open` / standard | 621.645 | 464.606 | 622.028 | 659.400 |
| `file.new` / standard | 2268.086 | 1791.422 | 2113.587 | 3391.210 |
| `file.open` / standard | 1030.914 | 1407.034 | 4337.484 | 12727.607 |
| `file.open_recent` / standard | 458.109 | 407.338 | 429.383 | 587.258 |
| `file.save` / standard | 581.482 | 840.699 | 4164.545 | 12616.350 |
| `file.save_as` / standard | 568.816 | 745.020 | 2867.185 | 11991.499 |
| `geometry.add_cuboid` / standard | 371.223 | 562.425 | 1966.869 | failed |
| `geometry.fill_loops` / standard | 347.442 | 663.783 | 2587.708 | failed |
| `geometry.flip_selection` / standard | 350.199 | 578.869 | 2612.859 | failed |
| `help.about` / standard | 382.487 | 437.489 | 416.426 | 504.708 |
| `hotspot.add` / standard | 323.794 | 614.514 | 2776.337 | failed |
| `hotspot.attach` / standard | 581.723 | 622.366 | 2606.467 | failed |
| `hotspot.begin_placement` / standard | 618.097 | 837.901 | 3442.515 | failed |
| `hotspot.cancel_placement` / standard | 388.568 | 363.789 | 1007.187 | 2861.528 |
| `hotspot.clear_reference` / standard | 296.649 | 416.201 | 1194.443 | failed |
| `hotspot.delete` / standard | 449.009 | 430.228 | 2707.433 | failed |
| `hotspot.reference` / standard | 529.819 | 752.262 | 3128.352 | failed |
| `hotspot.select_attached` / standard | 360.419 | 341.784 | 645.953 | failed |
| `hotspot.target` / standard | 378.065 | 540.157 | 2325.758 | failed |
| `hotspot.update` / standard | 394.637 | 634.514 | 2696.220 | failed |
| `import.mesh` / standard | 547.379 | 875.593 | 3118.397 | failed |
| `interchange.cancel` / standard | 379.061 | 386.355 | 487.653 | 794.722 |
| `interchange.formats` / standard | 333.668 | 439.852 | 385.828 | 1063.966 |
| `material.apply` / standard | 370.032 | 779.824 | 1749.045 | failed |
| `material.delete` / standard | 413.654 | 638.739 | 2461.389 | failed |
| `material.edit` / standard | 350.484 | 478.618 | 487.087 | 657.669 |
| `material.new` / standard | 399.659 | 420.645 | 360.906 | 679.159 |
| `material.new_brick` / standard | 405.400 | 419.963 | 456.202 | 801.061 |
| `material.new_foliage` / standard | 536.788 | 427.610 | 484.040 | 845.681 |
| `material.new_grass` / standard | 454.846 | 400.802 | 690.040 | 822.270 |
| `material.new_water` / standard | 423.524 | 483.793 | 417.037 | 839.066 |
| `material.new_wood` / standard | 544.757 | 472.346 | 578.005 | 862.981 |
| `material.parameters` / standard | 717.516 | 952.663 | 2686.425 | failed |
| `material.program` / standard | 482.950 | 834.660 | 2547.013 | failed |
| `material.save` / standard | 383.677 | 685.159 | 2261.266 | failed |
| `model.update_settings` / standard | 402.155 | 702.033 | 2126.775 | failed |
| `object.array_selection` / standard | 650.816 | 1481.813 | 8054.649 | failed |
| `object.configure_procedural` / standard | 1347.185 | 1837.971 | 4558.444 | failed |
| `object.create_from_selection` / standard | 355.924 | 589.269 | 2419.767 | failed |
| `object.delete_definition` / standard | 414.404 | 716.973 | 2047.355 | failed |
| `object.delete_unused_definition` / standard | 364.681 | 527.913 | 2030.369 | failed |
| `object.edit_tree` / standard | 964.354 | 624.028 | 1795.447 | failed |
| `object.explode_selection` / standard | 544.046 | 662.226 | 2566.386 | failed |
| `object.import_scene` / standard | 1277.438 | 1824.362 | 6903.463 | failed |
| `object.new_tree` / standard | 502.224 | 468.900 | 658.066 | 767.608 |
| `object.place_selected_definition` / standard | 1082.831 | 1394.195 | 3473.799 | failed |
| `object.reference_scene` / standard | 1576.483 | 1993.290 | 6644.730 | failed |
| `object.refresh_reference` / standard | 503.110 | 905.192 | 3672.162 | failed |
| `object.toggle_place_glue` / standard | 310.804 | 338.476 | 570.141 | 545.569 |
| `paint.apply_selection` / standard | 441.073 | 790.691 | 3225.960 | failed |
| `paint.set_color` / standard | 380.387 | 396.487 | 598.550 | 558.063 |
| `render.global_illumination` / standard | unsupported | unsupported | unsupported | unsupported |
| `render.raytrace` / standard | unsupported | unsupported | unsupported | unsupported |
| `render.save` / standard | unsupported | unsupported | unsupported | unsupported |
| `render.stop` / standard | unsupported | unsupported | unsupported | unsupported |
| `script.edit` / standard | 771.209 | 1141.186 | 2184.805 | 5333.927 |
| `script.new` / standard | 536.366 | failed | 1910.135 | failed |
| `selection.edit_object` / standard | 446.421 | 591.772 | 1216.057 | 3600.641 |
| `selection.exit_context` / standard | 452.664 | 456.922 | 1938.658 | 5814.102 |
| `selection.set` / standard | 555.014 | 591.846 | 1076.928 | 2082.087 |
| `selection.update_display` / standard | 331.400 | 343.972 | 1538.213 | 3368.783 |
| `tool.arch_slab_freeform` / standard | 2212.112 | 2635.896 | 4247.856 | failed |
| `tool.builtin.arch_add_hole` / standard | 1470.575 | 1701.064 | 3320.029 | failed |
| `tool.builtin.arch_door_frame` / standard | 1830.445 | 1897.357 | 3445.139 | failed |
| `tool.builtin.arch_slab` / standard | 1811.272 | 1759.674 | 3701.972 | failed |
| `tool.builtin.arch_stair` / standard | 629.479 | 885.044 | 2355.361 | failed |
| `tool.builtin.arch_wall` / standard | 1647.816 | 1757.082 | 4260.831 | failed |
| `tool.builtin.arch_window_frame` / standard | 1853.045 | 1960.924 | 3683.753 | failed |
| `tool.builtin.axial_grid` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.circle` / standard | 1679.713 | 2032.281 | 5023.849 | failed |
| `tool.builtin.construction_line` / standard | 1534.949 | 1972.318 | 4335.815 | failed |
| `tool.builtin.copy_multiple` / standard | 2584.686 | 2231.303 | 6253.665 | failed |
| `tool.builtin.cut_holes` / standard | 496.040 | 753.346 | 2841.692 | failed |
| `tool.builtin.cut_holes_2` / standard | 412.960 | 636.674 | 2514.634 | failed |
| `tool.builtin.cut_out_3` / standard | 453.556 | 803.932 | 2059.502 | failed |
| `tool.builtin.cut_with_plane` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.double_line` / standard | 2054.184 | 2639.378 | 4862.435 | failed |
| `tool.builtin.extrude_swipe` / standard | 2170.879 | 2272.827 | 4959.311 | failed |
| `tool.builtin.extrude_swipe_3d` / standard | 2864.404 | 2884.087 | 6132.964 | failed |
| `tool.builtin.face_outline` / standard | 499.327 | 813.760 | 2908.903 | failed |
| `tool.builtin.helical_array` / standard | 2544.356 | 2689.854 | 5972.523 | failed |
| `tool.builtin.helical_array_equalized` / standard | 2233.478 | 2535.340 | 5524.433 | failed |
| `tool.builtin.helicoidal_rotate_multiple` / standard | 2101.245 | 2882.896 | 5057.354 | failed |
| `tool.builtin.hvac_plumbing` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.hvac_ventilation` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.line` / standard | 1656.052 | 1814.558 | 4238.281 | failed |
| `tool.builtin.line_offset` / standard | 1798.168 | 1895.843 | 4046.392 | failed |
| `tool.builtin.linear_dimension` / standard | 1319.672 | 1470.856 | 3019.401 | failed |
| `tool.builtin.mech_circular_hole` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.mech_cog_wheel` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.mech_round_washer` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.mech_screw` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.mesh` / standard | 1548.004 | 2165.623 | 6328.897 | failed |
| `tool.builtin.mesh_intersection` / standard | 331.146 | 623.989 | 2071.024 | failed |
| `tool.builtin.mesh_regularize` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.move` / fragment | 1071.936 | 1404.437 | 2805.436 | failed |
| `tool.builtin.move` / standard | 990.265 | 1202.974 | 2239.802 | failed |
| `tool.builtin.object_cut` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.object_place` / standard | 834.997 | 1191.279 | 2503.676 | failed |
| `tool.builtin.paint` / standard | 439.045 | 515.608 | 420.225 | failed |
| `tool.builtin.planar_grid` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.planar_rotate_multiple` / standard | 1292.915 | 1954.942 | 3561.659 | failed |
| `tool.builtin.planar_translate_multiple` / standard | 1102.857 | 2360.778 | 6199.886 | failed |
| `tool.builtin.plane_section` / standard | 960.320 | 1437.044 | 2120.712 | failed |
| `tool.builtin.polyline` / standard | 1483.930 | 2105.896 | 3102.693 | failed |
| `tool.builtin.polyline_mesh` / standard | 318.247 | 697.006 | 2003.807 | failed |
| `tool.builtin.primitive_cone` / standard | 1242.277 | 1740.812 | 3256.542 | failed |
| `tool.builtin.primitive_cylinder` / standard | 1172.045 | 2038.022 | 2747.126 | failed |
| `tool.builtin.primitive_pill` / standard | 1379.281 | 1635.696 | 2668.199 | failed |
| `tool.builtin.primitive_sphere` / standard | 1043.761 | 1505.931 | 2254.010 | failed |
| `tool.builtin.push_pull` / standard | 1069.456 | 1560.941 | 2459.515 | failed |
| `tool.builtin.quad` / standard | 1422.855 | 1671.795 | 2600.779 | failed |
| `tool.builtin.random_offset` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.random_surface_array` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.rectangle` / standard | 1092.755 | 1633.594 | 2344.254 | failed |
| `tool.builtin.revolve` / standard | 1313.630 | 1800.532 | 2450.775 | failed |
| `tool.builtin.ribbon_3d` / standard | 1451.777 | 1911.354 | 2413.880 | failed |
| `tool.builtin.rotate` / fragment | 1580.000 | 2160.064 | 2686.815 | failed |
| `tool.builtin.rotate` / standard | 1275.504 | 1992.526 | 2225.772 | failed |
| `tool.builtin.rotate_2` / standard | 1373.586 | 1615.742 | 2076.942 | failed |
| `tool.builtin.rotate_stretch` / standard | 1527.719 | 2209.948 | 2239.860 | failed |
| `tool.builtin.rotational_array` / standard | 1663.990 | 2006.645 | 2950.272 | failed |
| `tool.builtin.rotational_array_equalized` / standard | 1381.616 | 2774.282 | 2912.839 | failed |
| `tool.builtin.scale` / standard | 2199.956 | 1650.520 | 2077.778 | failed |
| `tool.builtin.select` / standard | 632.958 | 655.789 | 541.045 | 1332.007 |
| `tool.builtin.solid_intersection` / standard | 412.923 | 723.807 | 1544.053 | failed |
| `tool.builtin.solid_subtraction` / standard | 566.093 | 706.788 | 1564.002 | failed |
| `tool.builtin.solid_union` / standard | 470.424 | 706.694 | 1477.328 | failed |
| `tool.builtin.stretch` / standard | 1154.477 | 1521.127 | 1887.527 | failed |
| `tool.builtin.stretch_scale` / standard | 1870.052 | 1662.162 | 1963.316 | failed |
| `tool.builtin.surface_rectangle` / standard | 1284.158 | 1609.941 | 2587.662 | failed |
| `tool.builtin.surface_remesh` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.text` / standard | 1147.803 | 1328.639 | 1834.522 | failed |
| `tool.builtin.vector_text` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.volumetric_translate_multiple` / standard | 1621.440 | 3524.465 | 12522.577 | failed |
| `tool.builtin.voxel` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.voxel_frame` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.builtin.voxel_volume` / standard | unsupported | unsupported | unsupported | unsupported |
| `tool.cancel` / standard | 451.219 | 434.672 | 255.069 | failed |
| `tool.copy_mode` / standard | 1416.030 | 2214.144 | 4589.856 | failed |
| `tool.cuboid` / standard | 1326.591 | 1683.478 | 2201.836 | failed |
| `tool.cycle.annotation` / standard | 1787.446 | 1907.370 | 2033.456 | failed |
| `tool.cycle.modification` / standard | 1558.984 | 1603.883 | 2054.568 | failed |
| `tool.cycle.operations` / standard | 1464.987 | 1701.635 | 2354.772 | failed |
| `tool.cycle.planar_construction` / standard | 802.928 | 830.401 | 652.361 | 1209.740 |
| `tool.distance` / standard | 1783.015 | 1978.569 | 4530.201 | failed |
| `tool.finish` / standard | 432.544 | 598.237 | 2357.550 | failed |
| `tool.leader` / standard | 1550.791 | 1566.477 | 1904.601 | failed |
| `tool.line` / standard | 1561.277 | 1382.660 | 2051.500 | failed |
| `tool.mirror` / standard | 1215.376 | 1551.326 | 1838.744 | failed |
| `tool.move_copy` / standard | 1305.130 | 1462.605 | 2058.941 | failed |
| `tool.rectangle` / standard | 1324.627 | 1599.700 | 2441.805 | failed |
| `tool.rotate_copy` / standard | 1572.047 | 1773.156 | 2286.675 | failed |
| `tool.select` / fragment | 556.260 | 670.267 | 633.178 | 1228.190 |
| `tool.select` / standard | 602.079 | 789.548 | 508.952 | 1138.449 |
| `tool.squash` / standard | 1469.874 | 1825.643 | 2124.572 | failed |
| `tool.undo_point` / standard | 379.380 | 359.855 | 258.498 | 348.664 |
| `ui.update_preferences` / standard | 659.172 | 459.729 | 245.535 | 334.960 |
| `view.architecture_settings` / standard | 533.365 | 442.334 | 301.401 | 370.245 |
| `view.axis_guide` / standard | 662.700 | 451.302 | 377.198 | 464.029 |
| `view.camera.orbit` / standard | 724.029 | 566.406 | 423.147 | 455.986 |
| `view.camera.orthographic` / standard | 688.212 | 464.776 | 391.116 | 443.549 |
| `view.camera.perspective` / standard | 744.493 | 476.206 | 423.734 | 459.105 |
| `view.camera.reset` / standard | 779.095 | 626.661 | 464.606 | 431.234 |
| `view.camera.set_pose` / standard | 806.178 | 582.746 | 389.033 | 433.657 |
| `view.camera.walkthrough` / standard | 598.278 | 572.177 | 641.473 | 1299.821 |
| `view.capture_ui_minimal` / standard | unsupported | unsupported | unsupported | unsupported |
| `view.capture_ui_restore` / standard | unsupported | unsupported | unsupported | unsupported |
| `view.frame_document` / standard | 595.964 | 512.704 | 583.601 | 1077.390 |
| `view.grid.toggle` / standard | 713.370 | 497.569 | 439.751 | 449.621 |
| `view.grid_guide` / standard | 642.241 | 534.759 | 381.657 | 458.542 |
| `view.guide.remove` / standard | 998.859 | 744.745 | 560.361 | 691.986 |
| `view.guides.clear` / standard | 667.945 | 603.137 | 383.506 | 438.838 |
| `view.guides.toggle` / standard | 686.719 | 634.154 | 403.205 | 456.961 |
| `view.help_about` / standard | 468.057 | 428.910 | 345.381 | 365.496 |
| `view.hotspot_settings` / standard | 415.033 | 317.707 | 262.689 | 323.456 |
| `view.hvac_settings` / standard | 406.011 | 327.680 | 275.967 | 330.359 |
| `view.lighting` / standard | 527.886 | 459.346 | 333.098 | 337.871 |
| `view.model_settings` / standard | 403.917 | 480.419 | 299.859 | 350.066 |
| `view.object_info` / standard | 357.368 | 405.886 | 266.557 | 338.251 |
| `view.objects` / standard | 379.409 | 522.964 | 276.027 | 338.888 |
| `view.orbit` / standard | 453.249 | 481.057 | 265.496 | 338.513 |
| `view.ortho.back` / standard | 740.363 | 471.140 | 270.985 | 342.171 |
| `view.ortho.bottom` / standard | 313.379 | 547.656 | 355.439 | 344.304 |
| `view.ortho.front` / standard | 462.786 | 566.630 | 267.440 | 343.474 |
| `view.ortho.left` / standard | 421.351 | 529.199 | 284.915 | 338.895 |
| `view.ortho.right` / standard | 325.068 | 443.800 | 289.803 | 333.367 |
| `view.ortho.top` / standard | 381.496 | 402.647 | 292.727 | 332.328 |
| `view.panel.architecture_frames.toggle` / standard | 558.506 | 598.533 | 347.889 | 380.692 |
| `view.panel.architecture_slabs.toggle` / standard | 466.563 | 643.657 | 395.610 | 438.302 |
| `view.panel.architecture_stairs.toggle` / standard | 538.790 | 833.521 | 400.835 | 423.811 |
| `view.panel.architecture_walls.toggle` / standard | 711.617 | 776.911 | 375.698 | 461.182 |
| `view.panel.help.toggle` / standard | 353.704 | 562.414 | 304.521 | 368.983 |
| `view.panel.hotspots.toggle` / standard | 313.638 | 444.719 | 276.231 | 310.843 |
| `view.panel.hvac.toggle` / standard | 335.234 | 521.363 | 270.070 | 329.998 |
| `view.panel.materials.toggle` / standard | 456.552 | 446.983 | 277.249 | 339.284 |
| `view.panel.mcp.toggle` / standard | 329.774 | 409.610 | 251.015 | 318.282 |
| `view.panel.model_settings.toggle` / standard | 330.352 | 513.140 | 293.396 | 325.265 |
| `view.panel.object.toggle` / standard | 419.450 | 436.059 | 263.016 | 327.340 |
| `view.panel.objects.toggle` / standard | 456.814 | 501.960 | 278.688 | 319.327 |
| `view.panel.polyline.toggle` / standard | 357.425 | 384.121 | 332.847 | 310.324 |
| `view.panel.profile.toggle` / standard | 412.586 | 582.343 | 276.200 | 340.930 |
| `view.panel.random.toggle` / standard | 344.887 | 535.350 | 270.035 | 325.548 |
| `view.panel.selection.toggle` / standard | 304.005 | 539.214 | 298.046 | 317.858 |
| `view.panel.shaders.toggle` / standard | 396.514 | 449.983 | 282.104 | 329.941 |
| `view.panel.tutorials.toggle` / standard | 315.125 | 442.734 | 282.141 | 313.070 |
| `view.panel.ui_settings.toggle` / standard | 541.686 | 409.688 | 323.012 | 333.478 |
| `view.panel.vector_text.toggle` / standard | 527.019 | 491.056 | 319.192 | 356.707 |
| `view.panels.toggle` / standard | 408.195 | 412.448 | 272.480 | 329.969 |
| `view.plugin_manager` / standard | unsupported | unsupported | unsupported | unsupported |
| `view.polyline_settings` / standard | 367.664 | 361.196 | 317.192 | 325.707 |
| `view.profile.buttons` / standard | 434.767 | 732.348 | 270.035 | 323.456 |
| `view.profile.hybrid` / standard | 432.201 | 446.834 | 302.640 | 348.336 |
| `view.profile.menus` / standard | 361.341 | 394.724 | 264.017 | 320.717 |
| `view.random_tool_settings` / standard | 433.698 | 368.920 | 273.194 | 339.455 |
| `view.selection` / standard | 437.102 | 469.824 | 257.902 | 337.761 |
| `view.toolbar.actions.hide` / standard | 491.039 | 473.163 | 309.287 | 324.307 |
| `view.toolbar.actions.show` / standard | 460.076 | 526.996 | 262.958 | 326.002 |
| `view.toolbar.advanced_mesh.hide` / standard | 378.560 | 375.371 | 286.122 | 331.906 |
| `view.toolbar.advanced_mesh.show` / standard | 489.725 | 426.046 | 286.636 | 322.951 |
| `view.toolbar.architecture.hide` / standard | 496.916 | 388.468 | 276.725 | 326.557 |
| `view.toolbar.architecture.show` / standard | 478.197 | 299.816 | 275.470 | 328.233 |
| `view.toolbar.arrays.hide` / standard | 487.681 | 310.706 | 269.990 | 327.421 |
| `view.toolbar.arrays.show` / standard | 385.701 | 357.749 | 279.713 | 324.014 |
| `view.toolbar.camera.hide` / standard | 517.079 | 301.325 | 269.287 | 516.861 |
| `view.toolbar.camera.show` / standard | 638.202 | 408.618 | 298.941 | 374.162 |
| `view.toolbar.construction_entities.hide` / standard | 406.634 | 272.884 | 270.564 | 329.868 |
| `view.toolbar.construction_entities.show` / standard | 404.107 | 306.425 | 275.819 | 364.417 |
| `view.toolbar.construction_points.hide` / standard | 511.494 | 289.400 | 304.389 | 362.533 |
| `view.toolbar.construction_points.show` / standard | 512.197 | 327.559 | 274.280 | 386.903 |
| `view.toolbar.hvac.hide` / standard | 411.216 | 290.922 | 286.404 | 389.904 |
| `view.toolbar.hvac.show` / standard | 484.734 | 253.635 | 496.832 | 602.857 |
| `view.toolbar.in_tool_operators.hide` / standard | 420.570 | 300.470 | 282.383 | 339.238 |
| `view.toolbar.in_tool_operators.show` / standard | 448.084 | 290.955 | 262.862 | 375.712 |
| `view.toolbar.mech.hide` / standard | 374.327 | 352.114 | 296.774 | 354.291 |
| `view.toolbar.mech.show` / standard | 410.091 | 273.507 | 265.651 | 323.595 |
| `view.toolbar.modification.hide` / standard | 417.742 | 257.683 | 277.887 | 328.248 |
| `view.toolbar.modification.show` / standard | 484.618 | 315.595 | 277.187 | 335.569 |
| `view.toolbar.modification_2.hide` / standard | 547.569 | 288.810 | 299.075 | 329.029 |
| `view.toolbar.modification_2.show` / standard | 404.649 | 332.306 | 250.028 | 330.731 |
| `view.toolbar.primitives.hide` / standard | 434.971 | 279.898 | 250.765 | 327.792 |
| `view.toolbar.primitives.show` / standard | 456.539 | 308.453 | 286.104 | 335.168 |
| `view.toolbar.random.hide` / standard | 364.092 | 345.342 | 275.461 | 333.363 |
| `view.toolbar.random.show` / standard | 523.912 | 308.298 | 267.363 | 328.942 |
| `view.toolbar.rendering.hide` / standard | 455.599 | 270.263 | 272.699 | 328.739 |
| `view.toolbar.rendering.show` / standard | 368.396 | 276.638 | 267.381 | 320.022 |
| `view.toolbar.solids.hide` / standard | 457.185 | 299.589 | 298.868 | 323.106 |
| `view.toolbar.solids.show` / standard | 412.097 | 270.439 | 268.094 | 337.696 |
| `view.toolbar.voxel.hide` / standard | 501.482 | 320.759 | 255.292 | 301.748 |
| `view.toolbar.voxel.show` / standard | 368.744 | 290.221 | 276.420 | 315.218 |
| `view.toolbars.toggle` / standard | 458.077 | 281.524 | 265.211 | 333.174 |
| `view.vector_text_settings` / standard | 388.799 | 337.870 | 315.643 | 357.418 |
| `view.workspace.reset_layout` / standard | 859.272 | 616.587 | 554.314 | 587.379 |

## Failures

- `script.new` / medium: Remote end closed connection without response
- `annotation.update` / heavy: Final pick did not commit the operation: undo history needs 451489484 snapshot bytes, over its 268435456 byte budget
- `architecture.add_hole` / heavy: undo history needs 451501226 snapshot bytes, over its 268435456 byte budget
- `architecture.create_frame` / heavy: undo history needs 451517323 snapshot bytes, over its 268435456 byte budget
- `architecture.create_rectangular_slab` / heavy: undo history needs 451501226 snapshot bytes, over its 268435456 byte budget
- `architecture.create_slab` / heavy: undo history needs 451501226 snapshot bytes, over its 268435456 byte budget
- `architecture.create_stair` / heavy: undo history needs 451877982 snapshot bytes, over its 268435456 byte budget
- `architecture.create_wall` / heavy: undo history needs 451506474 snapshot bytes, over its 268435456 byte budget
- `architecture.move_control` / heavy: undo history needs 451501226 snapshot bytes, over its 268435456 byte budget
- `architecture.move_slab_vertex` / heavy: undo history needs 451501226 snapshot bytes, over its 268435456 byte budget
- `architecture.remove_hole` / heavy: undo history needs 451501226 snapshot bytes, over its 268435456 byte budget
- `architecture.settings` / heavy: undo history needs 451491175 snapshot bytes, over its 268435456 byte budget
- `architecture.stair_path` / heavy: undo history needs 451877982 snapshot bytes, over its 268435456 byte budget
- `architecture.update` / heavy: undo history needs 451501226 snapshot bytes, over its 268435456 byte budget
- `document.snapshot` / heavy: request canceled or expired; query current revision before retrying
- `edit.cut_selection` / heavy: undo history needs 451488897 snapshot bytes, over its 268435456 byte budget
- `edit.delete_selection` / heavy: undo history needs 451488897 snapshot bytes, over its 268435456 byte budget
- `edit.duplicate_selection` / heavy: undo history needs 451489439 snapshot bytes, over its 268435456 byte budget
- `edit.group` / heavy: undo history needs 451490267 snapshot bytes, over its 268435456 byte budget
- `edit.paste` / heavy: Tool did not commit geometry: Paste: undo history needs 451489441 snapshot bytes, over its 268435456 byte budget
- `edit.paste_in_place` / heavy: undo history needs 451489439 snapshot bytes, over its 268435456 byte budget
- `edit.redo` / heavy: undo history needs 451497917 snapshot bytes, over its 268435456 byte budget
- `edit.undo` / heavy: undo history needs 451497917 snapshot bytes, over its 268435456 byte budget
- `edit.ungroup` / heavy: undo history needs 451498977 snapshot bytes, over its 268435456 byte budget
- `geometry.add_cuboid` / heavy: undo history needs 451497917 snapshot bytes, over its 268435456 byte budget
- `geometry.fill_loops` / heavy: undo history needs 451489961 snapshot bytes, over its 268435456 byte budget
- `geometry.flip_selection` / heavy: undo history needs 451489276 snapshot bytes, over its 268435456 byte budget
- `hotspot.add` / heavy: undo history needs 451489535 snapshot bytes, over its 268435456 byte budget
- `hotspot.attach` / heavy: undo history needs 451489535 snapshot bytes, over its 268435456 byte budget
- `hotspot.begin_placement` / heavy: undo history needs 451489537 snapshot bytes, over its 268435456 byte budget
- `hotspot.clear_reference` / heavy: undo history needs 451489535 snapshot bytes, over its 268435456 byte budget
- `hotspot.delete` / heavy: undo history needs 451489535 snapshot bytes, over its 268435456 byte budget
- `hotspot.reference` / heavy: undo history needs 451489535 snapshot bytes, over its 268435456 byte budget
- `hotspot.select_attached` / heavy: undo history needs 451489535 snapshot bytes, over its 268435456 byte budget
- `hotspot.target` / heavy: undo history needs 451489535 snapshot bytes, over its 268435456 byte budget
- `hotspot.update` / heavy: undo history needs 451489535 snapshot bytes, over its 268435456 byte budget
- `import.mesh` / heavy: {'dispatch': {'command_id': 'import.mesh', 'accepted': True, 'result': {'completion': {'state': 'failed'}, 'document_revision': None, 'outputs': {}, 'diagnostics': ['undo history needs 451490692 snapshot bytes, over its 268435456 byte budget'], 'host_effects': []}}, 'status': {'prototypeDeletion': None, 'pendingDialog': None, 'history': {'cursor': 0, 'entries': 0}, 'version': '0.40.0', 'revision': 52, 'generation': 1, 'activeTool': 'Select', 'numericInput': '', 'toolInstructions': '', 'status': 'Importing /home/alfu64/Development/meshcraft/target/stress-final-heavy/work/heavy/import.obj… Use File > Cancel Import / Export to stop.', 'editContext': {'kind': 'document'}, 'selection': [], 'projectPath': '/home/alfu64/Development/meshcraft/target/stress-final-heavy/work/heavy/output.octd', 'unsavedChanges': False, 'clipboard': {'available': False, 'bytes': 0}}, '_timing': {'ownerThreadMs': 0.911583}}
- `material.apply` / heavy: undo history needs 451499242 snapshot bytes, over its 268435456 byte budget
- `material.delete` / heavy: undo history needs 451489448 snapshot bytes, over its 268435456 byte budget
- `material.parameters` / heavy: undo history needs 451489604 snapshot bytes, over its 268435456 byte budget
- `material.program` / heavy: undo history needs 451490997 snapshot bytes, over its 268435456 byte budget
- `material.save` / heavy: undo history needs 451489448 snapshot bytes, over its 268435456 byte budget
- `model.update_settings` / heavy: undo history needs 451489276 snapshot bytes, over its 268435456 byte budget
- `object.array_selection` / heavy: request canceled or expired; query current revision before retrying
- `object.configure_procedural` / heavy: undo history needs 451490102 snapshot bytes, over its 268435456 byte budget
- `object.create_from_selection` / heavy: undo history needs 451490267 snapshot bytes, over its 268435456 byte budget
- `object.delete_definition` / heavy: undo history needs 451489865 snapshot bytes, over its 268435456 byte budget
- `object.delete_unused_definition` / heavy: undo history needs 451489865 snapshot bytes, over its 268435456 byte budget
- `object.edit_tree` / heavy: undo history needs 451490102 snapshot bytes, over its 268435456 byte budget
- `object.explode_selection` / heavy: undo history needs 451498977 snapshot bytes, over its 268435456 byte budget
- `object.import_scene` / heavy: {'dispatch': {'command_id': 'object.import_scene', 'accepted': True, 'result': {'completion': {'state': 'failed'}, 'document_revision': None, 'outputs': {}, 'diagnostics': ['undo history needs 451503563 snapshot bytes, over its 268435456 byte budget'], 'host_effects': []}}, 'status': {'prototypeDeletion': None, 'pendingDialog': None, 'history': {'cursor': 0, 'entries': 0}, 'version': '0.40.0', 'revision': 52, 'generation': 1, 'activeTool': 'Select', 'numericInput': '', 'toolInstructions': '', 'status': 'Loading object scene /home/alfu64/Development/meshcraft/examples/dynamic-object.octd…', 'editContext': {'kind': 'document'}, 'selection': [{'path': [1332349, 1332340], 'kind': 'occurrence', 'id': None}], 'projectPath': '/home/alfu64/Development/meshcraft/target/stress-final-heavy/work/heavy/output.octd', 'unsavedChanges': False, 'clipboard': {'available': False, 'bytes': 0}}, '_timing': {'ownerThreadMs': 0.53396}}
- `object.place_selected_definition` / heavy: Final pick did not commit the operation: undo history needs 451489441 snapshot bytes, over its 268435456 byte budget
- `object.reference_scene` / heavy: {'dispatch': {'command_id': 'object.reference_scene', 'accepted': True, 'result': {'completion': {'state': 'failed'}, 'document_revision': None, 'outputs': {}, 'diagnostics': ['undo history needs 451503625 snapshot bytes, over its 268435456 byte budget'], 'host_effects': []}}, 'status': {'prototypeDeletion': None, 'pendingDialog': None, 'history': {'cursor': 0, 'entries': 0}, 'version': '0.40.0', 'revision': 52, 'generation': 1, 'activeTool': 'Select', 'numericInput': '', 'toolInstructions': '', 'status': 'Loading object scene /home/alfu64/Development/meshcraft/examples/dynamic-object.octd…', 'editContext': {'kind': 'document'}, 'selection': [{'path': [1332349, 1332340], 'kind': 'occurrence', 'id': None}], 'projectPath': '/home/alfu64/Development/meshcraft/target/stress-final-heavy/work/heavy/output.octd', 'unsavedChanges': False, 'clipboard': {'available': False, 'bytes': 0}}, '_timing': {'ownerThreadMs': 0.9276949999999999}}
- `object.refresh_reference` / heavy: {'dispatch': {'command_id': 'object.reference_scene', 'accepted': True, 'result': {'completion': {'state': 'failed'}, 'document_revision': None, 'outputs': {}, 'diagnostics': ['undo history needs 451503625 snapshot bytes, over its 268435456 byte budget'], 'host_effects': []}}, 'status': {'prototypeDeletion': None, 'pendingDialog': None, 'history': {'cursor': 0, 'entries': 0}, 'version': '0.40.0', 'revision': 52, 'generation': 1, 'activeTool': 'Select', 'numericInput': '', 'toolInstructions': '', 'status': 'Loading object scene /home/alfu64/Development/meshcraft/examples/dynamic-object.octd…', 'editContext': {'kind': 'document'}, 'selection': [{'path': [1332349, 1332340], 'kind': 'occurrence', 'id': None}], 'projectPath': '/home/alfu64/Development/meshcraft/target/stress-final-heavy/work/heavy/output.octd', 'unsavedChanges': False, 'clipboard': {'available': False, 'bytes': 0}}, '_timing': {'ownerThreadMs': 0.6056600000000001}}
- `paint.apply_selection` / heavy: undo history needs 451489268 snapshot bytes, over its 268435456 byte budget
- `script.new` / heavy: Remote end closed connection without response
- `tool.arch_slab_freeform` / heavy: undo history needs 451498007 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.arch_add_hole` / heavy: undo history needs 451501226 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.arch_door_frame` / heavy: Final pick did not commit the operation: undo history needs 451515716 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.arch_slab` / heavy: Final pick did not commit the operation: undo history needs 451501244 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.arch_stair` / heavy: undo history needs 451877982 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.arch_wall` / heavy: Final pick did not commit the operation: undo history needs 451506481 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.arch_window_frame` / heavy: Final pick did not commit the operation: undo history needs 451517325 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.circle` / heavy: Final pick did not commit the operation: undo history needs 451506838 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.construction_line` / heavy: Final pick did not commit the operation: undo history needs 451489724 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.copy_multiple` / heavy: Final pick did not commit the operation: undo history needs 451489765 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.cut_holes` / heavy: Final pick did not commit the operation: undo history needs 451489961 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.cut_holes_2` / heavy: Final pick did not commit the operation: undo history needs 451489961 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.cut_out_3` / heavy: Final pick did not commit the operation: undo history needs 451489961 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.double_line` / heavy: undo history needs 451492830 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.extrude_swipe` / heavy: undo history needs 451499929 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.extrude_swipe_3d` / heavy: undo history needs 451500069 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.face_outline` / heavy: undo history needs 451489276 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.helical_array` / heavy: Final pick did not commit the operation: undo history needs 451489680 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.helical_array_equalized` / heavy: Final pick did not commit the operation: undo history needs 451489680 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.helicoidal_rotate_multiple` / heavy: Final pick did not commit the operation: undo history needs 451489597 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.line` / heavy: Final pick did not commit the operation: undo history needs 451489724 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.line_offset` / heavy: Final pick did not commit the operation: undo history needs 451493469 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.linear_dimension` / heavy: Final pick did not commit the operation: undo history needs 451489471 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.mesh` / heavy: Final pick did not commit the operation: undo history needs 451489724 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.mesh_intersection` / heavy: undo history needs 451493456 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.move` / heavy: Final pick did not commit the operation: undo history needs 451489276 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.move` / heavy: Final pick did not commit the operation: undo history needs 451487836 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.object_place` / heavy: Final pick did not commit the operation: undo history needs 451489441 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.paint` / heavy: request canceled or expired; query current revision before retrying
- `tool.builtin.planar_rotate_multiple` / heavy: Final pick did not commit the operation: undo history needs 451489680 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.planar_translate_multiple` / heavy: request canceled or expired; query current revision before retrying
- `tool.builtin.plane_section` / heavy: Final pick did not commit the operation: undo history needs 451492172 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.polyline` / heavy: undo history needs 451490088 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.polyline_mesh` / heavy: undo history needs 451489961 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.primitive_cone` / heavy: Final pick did not commit the operation: undo history needs 451522488 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.primitive_cylinder` / heavy: Final pick did not commit the operation: undo history needs 451548462 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.primitive_pill` / heavy: Final pick did not commit the operation: undo history needs 451894066 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.primitive_sphere` / heavy: Final pick did not commit the operation: undo history needs 451861578 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.push_pull` / heavy: Final pick did not commit the operation: undo history needs 451495386 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.quad` / heavy: Final pick did not commit the operation: undo history needs 451491417 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.rectangle` / heavy: Final pick did not commit the operation: undo history needs 451491418 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.revolve` / heavy: undo history needs 451619905 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.ribbon_3d` / heavy: undo history needs 451493351 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.rotate` / heavy: Final pick did not commit the operation: undo history needs 451489313 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.rotate` / heavy: Final pick did not commit the operation: undo history needs 451635088 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.rotate_2` / heavy: Final pick did not commit the operation: undo history needs 451489313 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.rotate_stretch` / heavy: Final pick did not commit the operation: undo history needs 451489313 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.rotational_array` / heavy: Final pick did not commit the operation: undo history needs 451489680 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.rotational_array_equalized` / heavy: Final pick did not commit the operation: undo history needs 451489680 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.scale` / heavy: Final pick did not commit the operation: undo history needs 451489276 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.solid_intersection` / heavy: undo history needs 451510486 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.solid_subtraction` / heavy: undo history needs 451526020 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.solid_union` / heavy: undo history needs 451552378 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.stretch` / heavy: Final pick did not commit the operation: undo history needs 451489276 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.stretch_scale` / heavy: Final pick did not commit the operation: undo history needs 451489276 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.surface_rectangle` / heavy: Final pick did not commit the operation: undo history needs 451491418 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.text` / heavy: Final pick did not commit the operation: undo history needs 451489484 snapshot bytes, over its 268435456 byte budget
- `tool.builtin.volumetric_translate_multiple` / heavy: request canceled or expired; query current revision before retrying
- `tool.cancel` / heavy: request canceled or expired; query current revision before retrying
- `tool.copy_mode` / heavy: undo history needs 451489439 snapshot bytes, over its 268435456 byte budget
- `tool.cuboid` / heavy: Final pick did not commit the operation: undo history needs 451497714 snapshot bytes, over its 268435456 byte budget
- `tool.cycle.annotation` / heavy: Final pick did not commit the operation: undo history needs 451489471 snapshot bytes, over its 268435456 byte budget
- `tool.cycle.modification` / heavy: Final pick did not commit the operation: undo history needs 451489276 snapshot bytes, over its 268435456 byte budget
- `tool.cycle.operations` / heavy: Final pick did not commit the operation: undo history needs 451493469 snapshot bytes, over its 268435456 byte budget
- `tool.distance` / heavy: undo history needs 451489754 snapshot bytes, over its 268435456 byte budget
- `tool.finish` / heavy: undo history needs 451489724 snapshot bytes, over its 268435456 byte budget
- `tool.leader` / heavy: Final pick did not commit the operation: undo history needs 451489557 snapshot bytes, over its 268435456 byte budget
- `tool.line` / heavy: Final pick did not commit the operation: undo history needs 451489724 snapshot bytes, over its 268435456 byte budget
- `tool.mirror` / heavy: Final pick did not commit the operation: undo history needs 451489277 snapshot bytes, over its 268435456 byte budget
- `tool.move_copy` / heavy: Final pick did not commit the operation: undo history needs 451489439 snapshot bytes, over its 268435456 byte budget
- `tool.rectangle` / heavy: Final pick did not commit the operation: undo history needs 451491418 snapshot bytes, over its 268435456 byte budget
- `tool.rotate_copy` / heavy: Final pick did not commit the operation: undo history needs 451489476 snapshot bytes, over its 268435456 byte budget
- `tool.squash` / heavy: Final pick did not commit the operation: undo history needs 451489276 snapshot bytes, over its 268435456 byte budget

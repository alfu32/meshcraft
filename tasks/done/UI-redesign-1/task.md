# UI redesign: title bar, ribbon and left panel composition

Status: **decisions made, in implementation.**
Baseline: 0.49.0 (`58c7f07`). Target version: 0.50.0.

This records the request as read from the seven annotated screenshots in this
directory. Section 3 records what they didn't settle: your answers, and the
defaults used for the smaller points.

## 1. What the screenshots ask for

| Screenshot | Annotation | Request |
| --- | --- | --- |
| `01_files-and-system-level-functions.1.png` | "move them in the title bar"; Files tab and its Projects group circled; left panel Files sections crossed out | Files functions move to the title bar |
| `01_files-and-system-level-functions.2.png` | "move to title bar together with files, system-wide-functions"; Clipboard & History tab and group circled; left panel section boxed | Clipboard & History moves to the title bar, beside Files |
| `01_files-and-system-level-functions.3.png` | "remove from toolbars/ribbon, move to the titlebar"; Clipboard & History crossed out on the Home ribbon; left panel Files · Projects, Files · Import & Export and Clipboard & History crossed out | Those commands leave the Home ribbon and the left panel |
| `01_primary-construction-functions.1.png` | "Selection, Draw, Annotations, Transform and Guides go to the Home ribbon (in this order)" | Five groups move from Construction to Home, in that order |
| `02_primary-construction-functions.2.png` | "input and settings go to the settings ribbon" | Input & Settings moves from Construction to Settings |
| `03_meshing-and-modify-functions.1.png` | "meshing and modify go together to Construction"; Meshing and Modify tabs circled | Both tabs merge into Construction |
| `04_objects-and-hotspots-merged-toghether.png` | "objects and hotspots ribbons should be merged together" | Both tabs become one |

The arrows end just right of the title bar's Home icon, so the new commands go
between that icon and the title text.

## 2. Target layout

### Title bar

```
[logo] [Home] | New Open Recent Save SaveAs CancelOpen Exit | Import ExportMesh ExportIFC ExportSVG Screenshot CancelImport | Undo Redo Cut Copy Paste PasteInPlace Duplicate Delete | Meshcraft 0.50.0 /path/file.octd   [_][□][x]
```

- Compact icon buttons. The tooltip gives the command label and shortcut.
- The Home toggle stays where it is. It is `file.home`, so it isn't added twice.
- That makes 21 new buttons plus Home. At about 26 px each they need roughly
  570 px, which leaves about 130 px for the title at the 900 px minimum window
  width. The title truncates first. Measure this during implementation.

### Ribbon tabs

Before: Home, Files, Clipboard & History, Construction, Meshing, Modify, Solids,
Civil, Materials, Objects, Hotspots, Settings (12).

After: **Home, Construction, Solids, Civil, Materials, Objects & Hotspots,
Settings** (7).

| Tab | Groups, in order | Change |
| --- | --- | --- |
| Home | Selection, Draw, Annotations, Transform, Guides, then the two pickers, Mesh, Hotspots, Solids | Takes the five full groups from Construction. Clipboard & History is removed |
| Construction | Construct, Advanced (Meshing), Modify, Arrays (Modify) | Holds only the former Meshing and Modify content |
| Solids, Civil, Materials | Unchanged | None |
| Objects & Hotspots | Prototypes, Instances & Groups; Scripts & GPU; External Prototypes; Hotspots | Merged |
| Settings | Input & Settings, Model & Interface, Camera, Workspace Profiles, Automation & Help | Gains Input & Settings |

Each group moves whole. The screenshot shows only four Transform buttons because
the strip is clipped there; all twelve move.

### Left panel

Remove Files · Projects, Files · Import & Export and Clipboard & History. The
panel then starts at Selection. Everything else stays, including the search field
and resizing.

### Menu row

Remove the Files and Clipboard & History dropdowns, so the row starts at Selection.
The Recent Files list they offered must stay reachable from the title bar.

### Availability

Command IDs, handlers, shortcuts and availability rules don't change. All 292
registered commands stay in the command palette and MCP.

## 3. Decisions

### Your answers

- **Home ribbon:** keep everything you didn't cross out. After the five groups come
  the Selection Material and Insert Object pickers, then the Mesh, Hotspots and
  Solids shortcuts. Home may scroll horizontally.
- **Menu row:** remove the Files and Clipboard & History dropdowns. All other
  menus and the remaining left panel groups stay as they are.
- **Import & Export:** moves to the title bar, as in the layout above.

### Defaults I'll use unless you object

| Point | Default | Reason |
| --- | --- | --- |
| Merged tab name | "Objects & Hotspots" | Matches the naming of "Clipboard & History" |
| Input & Settings position | First on Settings | Finish, Undo Point and Cancel stay visible without scrolling |
| Construction group order | Construct, Advanced, Modify, Arrays | Follows the old tab order |
| Exit and the two Cancel commands | Moved as circled, greyed out when unavailable | Literal reading. Exit does overlap with the window close button |
| While the Home screen is open | Title bar shows only the Home toggle | The Home sidebar covers these actions, and editing commands have no visible target |

## 4. Risks to handle

1. **Modal editors.** The script, shader, material and IFC editors start below the
   title bar, so its buttons stay clickable while they are open. Those commands
   were never reachable there before, so Open or Undo could act behind an editor.
   The title bar commands must be disabled while a modal is open.
2. **Clipboard path.** Cut, Copy and Paste must go through the same dispatch as the
   ribbon buttons, which syncs the system clipboard. They can't use a plain
   `dispatch`.
3. **Window dragging.** Buttons sit inside the draggable title bar, so a click must
   not start a window drag.
4. **Button visibility.** The title bar commands must stay visible when Show
   Toolbars, the ribbon or the left panel is switched off. Otherwise a saved
   workspace could hide Save and Undo completely.
5. **Long labels on Home.** Three-row groups have short rows. Labels such as "Clear
   Construction Guides" and "Selection Properties" may clip, so check them in a
   capture.
6. **Lost search results.** The left panel search, including its Enter action, will
   no longer find Files and Clipboard commands. The palette will. This follows from
   the request but is a visible change.

## 5. Work involved

| Area | Change |
| --- | --- |
| `apps/desktop/src/functional_ui.rs` | Add a tab-to-groups table so that ribbon composition no longer depends on `root`; add the title bar groups; new `RIBBON_TABS` and `HOME` |
| `apps/desktop/src/gpui_app.rs` | Title bar buttons; ribbon and modal handling |
| `apps/desktop/src/gpui_layout.rs` | Left panel filter; move the pickers (now hardcoded after Home's first group) |
| `apps/desktop/src/gpui_workspace_controls.rs` | Report the title bar buttons to MCP, as other controls are |
| Tests in `gpui_app.rs` | Rewrite the ribbon projection test; add coverage for every title bar command and for tab order |
| `scripts/verify_functional_ui.py` | New tab list and title bar checks, with evidence under `docs/rewrite/references/` |
| `scripts/capture_store_features.py`, `scripts/capture_store_projects.py` | They use old tab names and hardcoded tab positions |
| Docs | `docs/menu-layout.md`, `docs/rewrite/spec/interface.md`, `docs/rewrite/spec/modeling.md`, `docs/rewrite/acceptance.md`, `docs/rewrite/tasks.json`, a handoff, `README.md`, `DASHBOARD.md` |
| Screenshots | Refresh per `docs/screenshots/manifest.json` |
| Inventory and stress | No change expected, because no command is added or renamed. Confirm with `export_tool_inventory.py --check` |

## 6. Done when

- The title bar shows the buttons above in order. Each one runs its command and
  matches the availability of that command in the palette.
- The ribbon has exactly the seven tabs, with the groups in the order above and
  no command dropped from any moved group.
- The left panel and the menu row both start at Selection.
- Every command that was on the ribbon before is still reachable from the title
  bar or the ribbon. A test checks this.
- With a modal open, title bar commands do nothing. With the ribbon and left panel
  hidden, the title bar commands remain.
- Layout is checked at 900 px and at 1880 px wide, in the real application over
  MCP under Xvfb.
- The release gate passes: `cargo fmt --all --check`,
  `cargo test --workspace --locked`, and
  `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `verify_plan.py` and `export_tool_inventory.py --check` pass.

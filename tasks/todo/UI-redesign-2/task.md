# UI redesign 2: Home ribbon cleanup and title bar command search

Status: **done, released as 0.51.0.** Baseline: 0.50.0. Target version: 0.51.0.

Read from the two annotated screenshots in this directory.

## 1. Home ribbon cleanup (`05_cleanup-home-ribbon.png`)

Annotation: "we dont need those in home", with the Mesh, Hotspots and Solids groups
crossed out.

Remove those three groups from Home. Home then holds Selection, Draw, Annotations,
Transform, Guides, and the Selection Material and Insert Object pickers. Their
commands keep their buttons on the Construction, Objects & Hotspots and Solids tabs.

## 2. Title bar command search (`06_redesign-command-search-box.png`)

Annotation: "remove the button, have a search box in the title bar (aligned right)
accompanied by: a search button that searches through commands, a chevron button
that shows/hides the list of commands. By default we do not show the dropdown menus,
only the ribbon and the left panel."

- Remove the "Commands Ctrl+Shift+P" button from the menu row.
- Add a right-aligned cluster on the title bar, before the window controls: search
  box, search button, chevron button.
- Search button: runs the search for the box's text and shows the matching commands.
- Chevron: shows or hides the command list, and points up while the list is open.
- The menu bar is hidden by default. The ribbon and left panel stay shown.

### Defaults used where the screenshot doesn't say

| Point | Default | Reason |
| --- | --- | --- |
| Typing in the box | Opens the list and filters as you type | Same as the palette today |
| Ctrl+Shift+P | Kept: focuses the box and opens the list | The shortcut is documented and users know it |
| Keyboard in the list | Up, Down, Enter and Escape as in the palette today | No behavior should be lost |
| List position | Directly below the cluster, right-aligned | It belongs to the box |
| Modal editors and prompts | Cluster disabled, like the other title bar buttons | Commands must not act behind an editor |
| Home screen | Cluster stays enabled | The Commands button was reachable on Home, so disabling it would remove something; running a workspace command leaves Home |
| Clicking into the box | Opens the list, so typed letters never reach tool hotkeys | Same protection the palette has today |
| Narrow windows | The box shrinks before the title disappears, down to a usable minimum | The title bar is already full at 900 px |
| Existing saved preferences | A saved "Show menu bar" setting is kept | Only the default changes; changing someone's saved choice would be a surprise. UI Settings > Show menu bar turns it off |

## Done when

- Home has exactly the groups above, and no placed command loses its button.
- The Commands button is gone and the cluster is on the title bar, right-aligned.
- Typing, the search button, the chevron, Up/Down, Enter, Escape and Ctrl+Shift+P all
  work in the real application.
- A fresh profile starts without the menu bar, with the ribbon and left panel shown.
- The buttons and the title fit at 900 px and 1880 px wide, and window controls stay
  visible.
- The release gate passes, then commit, tag, push and trigger the release workflow.

# Project materials

### P07 Material catalog and color rendering

**New user requirement (2026-10-20).** Each project exposes a Materials inspector with a single Default material initially. Users can create additional materials, see their color swatches and names, select them, and open a material editor by double-click or Edit. Preserve text labels on controls.

A material has a stable ID, name, RGBA color including transparent alpha, shading type (Unlit, Flat, Gouraud or Phong initially), optional texture path, and positive finite texture U/V sizes in model units. Defaults are an opaque neutral color, Phong shading, no texture and unit U/V sizes. The first rendering phase uses material colors/alpha with existing viewport lighting; texture mapping and distinct shading algorithms are retained metadata, not implemented rendering promises.

Save validates and commits one undoable edit. Cancel/Escape discards the draft. Modal pointer, wheel and keyboard input cannot reach modeling tools or camera navigation. Apply to Selection binds editable selected edges/faces or selected instance content to the material, respecting prototype editing context and independent instances. Later edits to a referenced material update its displayed color. Direct color painting replaces the material reference. Existing imported colors remain intact until explicitly assigned; new model geometry uses the Default material. A material catalog and its references survive project IO and history; scene import remaps conflicting material IDs without recoloring the host catalog.

Source: new user contract; existing [object workflow](objects.md), paint and project/history APIs. Acceptance: C42. Implementation: T61. Texture evaluation remains future work.

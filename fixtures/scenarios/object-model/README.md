# Object-model migration cases

These fixtures pin the T46 normalization decisions to concrete legacy inputs. Assertion paths are JSON Pointers into the normalized projection; `definitions/by_legacy_id/<id>`, `definitions/by_name/<name>`, and `occurrences/by_legacy_id/<id>` select records through preserved source aliases. Exact corner coordinates and identity IDs compare separately.

The alias projection groups refs by the attached `LegacyAlias`; `vertex_id_count` counts distinct topological IDs and `corner_positions` lists sorted, resolved points. Occurrence `geometry_source` is `definition` when inherited and `instance_override` for a retained replacement.

The end-to-end scenario runner is part of the later IO/editor work. Until then, the Rust importer tests named in `docs/rewrite/handoffs/T46.md` execute the alias, mixed-domain and differing-override cases. Keep fixture hashes aligned with `docs/rewrite/source-inventory.json` and preserve the source files as read-only evidence.

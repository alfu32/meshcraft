//! Functional homes for native menus, ribbons and the left command panel.
//! Compatibility aliases and parameter-only automation endpoints stay in the registry.
use meshcraft_core::commands::CommandDescriptor;

pub(crate) const GROUPS: &[&str] = &[
    "2D Construction",
    "Meshing",
    "Solids",
    "Civil",
    "Materials",
    "Objects",
    "Files",
    "Settings",
];
pub(crate) struct Section {
    pub root: &'static str,
    pub title: &'static str,
    pub commands: &'static [(&'static str, &'static str)],
}
pub(crate) const SECTIONS: &[Section] = &[
    Section {
        root: "2D Construction",
        title: "Selection",
        commands: &[
            ("tool.select", "Select"),
            ("edit.clear_selection", "Clear Selection"),
            ("view.selection", "Selection Properties"),
        ],
    },
    Section {
        root: "2D Construction",
        title: "Clipboard & History",
        commands: &[
            ("edit.undo", "Undo"),
            ("edit.redo", "Redo"),
            ("edit.cut_selection", "Cut"),
            ("edit.copy_selection", "Copy"),
            ("edit.paste", "Paste"),
            ("edit.paste_in_place", "Paste in Place"),
            ("edit.duplicate_selection", "Duplicate Selection"),
            ("edit.delete_selection", "Delete Selection"),
        ],
    },
    Section {
        root: "2D Construction",
        title: "Draw",
        commands: &[
            ("tool.builtin.line", "Line"),
            ("tool.builtin.polyline", "Polyline"),
            ("tool.builtin.construction_line", "Construction Line"),
            ("tool.builtin.double_line", "Double Line"),
            ("tool.builtin.rectangle", "Rectangle"),
            ("tool.builtin.circle", "Circle"),
        ],
    },
    Section {
        root: "2D Construction",
        title: "Annotations",
        commands: &[
            ("tool.builtin.linear_dimension", "Linear Dimension"),
            ("tool.builtin.text", "Text"),
            ("tool.builtin.vector_text", "Vector Text"),
            ("tool.leader", "Leader"),
        ],
    },
    Section {
        root: "2D Construction",
        title: "Guides",
        commands: &[
            ("view.axis_guide", "Add Axis Guide"),
            ("view.grid_guide", "Add Grid Guide"),
            ("view.guide.remove", "Remove Guide"),
            ("view.guides.clear", "Clear Construction Guides"),
            ("view.guides.toggle", "Show Guides"),
            ("view.grid.toggle", "Show Grid"),
        ],
    },
    Section {
        root: "2D Construction",
        title: "Input & Settings",
        commands: &[
            ("tool.distance", "Distance Input"),
            ("tool.finish", "Finish Current Input"),
            ("tool.undo_point", "Undo Last Point or Input"),
            ("tool.cancel", "Cancel Current Tool"),
            ("tool.copy_mode", "Toggle Copy Mode"),
            ("view.polyline_settings", "Polyline Settings"),
            ("view.vector_text_settings", "Annotation Settings"),
        ],
    },
    Section {
        root: "Meshing",
        title: "Construct",
        commands: &[
            ("tool.builtin.mesh", "Planar Mesh"),
            ("tool.builtin.polyline_mesh", "Polyline Mesh"),
            ("tool.builtin.ribbon_3d", "Mesh Two Lines"),
            ("tool.builtin.quad", "Quad"),
            ("tool.builtin.surface_rectangle", "Surface Rect"),
            ("tool.builtin.face_outline", "Face Outline"),
            ("geometry.fill_loops", "Fill Loops"),
            ("tool.builtin.push_pull", "Push/Pull"),
            ("tool.builtin.extrude_swipe", "Sweep"),
            ("tool.builtin.extrude_swipe_3d", "Sweep 3D"),
            ("tool.builtin.revolve", "Revolve"),
        ],
    },
    Section {
        root: "Meshing",
        title: "Transform",
        commands: &[
            ("tool.builtin.move", "Move"),
            ("tool.move_copy", "Move Copy"),
            ("tool.builtin.rotate", "Rotate"),
            ("tool.builtin.rotate_2", "Rotate by Two Vectors"),
            ("tool.rotate_copy", "Rotate Copy"),
            ("tool.builtin.scale", "Scale"),
            ("tool.squash", "Squash"),
            ("tool.builtin.stretch", "Stretch"),
            ("tool.builtin.stretch_scale", "Stretch Scale"),
            ("tool.builtin.rotate_stretch", "Rotate Stretch"),
            ("tool.mirror", "Mirror Selection"),
            ("geometry.flip_selection", "Flip Direction / Normals"),
        ],
    },
    Section {
        root: "Meshing",
        title: "Modify",
        commands: &[
            ("tool.builtin.line_offset", "Line Offset"),
            ("tool.builtin.cut_out_3", "Cutout"),
            ("tool.builtin.cut_with_plane", "Cut With Plane"),
            ("tool.builtin.cut_holes", "Cut Holes"),
            ("tool.builtin.cut_holes_2", "Cut Holes 2"),
        ],
    },
    Section {
        root: "Meshing",
        title: "Arrays",
        commands: &[
            ("tool.builtin.copy_multiple", "Copy Multiple"),
            (
                "tool.builtin.planar_translate_multiple",
                "Planar Translate Multiple",
            ),
            (
                "tool.builtin.volumetric_translate_multiple",
                "Volumetric Translate Multiple",
            ),
            ("tool.builtin.rotational_array", "Rotational Array"),
            (
                "tool.builtin.rotational_array_equalized",
                "Rotational Array Even",
            ),
            ("tool.builtin.helical_array", "Helical Array"),
            ("tool.builtin.helical_array_equalized", "Helical Array Even"),
            ("tool.builtin.random_surface_array", "Random Surface Array"),
        ],
    },
    Section {
        root: "Meshing",
        title: "Advanced",
        commands: &[
            ("tool.builtin.mesh_regularize", "Planar Regularize"),
            ("tool.builtin.surface_remesh", "Surface Remesh"),
            ("tool.builtin.random_offset", "Random Offset"),
            ("view.random_tool_settings", "Random Tool Settings"),
        ],
    },
    Section {
        root: "Solids",
        title: "Primitives",
        commands: &[
            ("tool.cuboid", "Cuboid"),
            ("tool.builtin.primitive_sphere", "Sphere"),
            ("tool.builtin.primitive_cylinder", "Cylinder"),
            ("tool.builtin.primitive_cone", "Cone"),
            ("tool.builtin.primitive_pill", "Pill"),
        ],
    },
    Section {
        root: "Solids",
        title: "Booleans & Sections",
        commands: &[
            ("tool.builtin.solid_union", "Union"),
            ("tool.builtin.solid_subtraction", "Difference"),
            ("tool.builtin.solid_intersection", "Intersection"),
            ("tool.builtin.mesh_intersection", "Intersection Curves"),
            ("tool.builtin.plane_section", "Planar Section"),
        ],
    },
    Section {
        root: "Solids",
        title: "Mechanical",
        commands: &[
            ("tool.builtin.mech_cog_wheel", "Cog Wheel"),
            ("tool.builtin.mech_round_washer", "Round Washer"),
            ("tool.builtin.mech_screw", "Screw"),
            ("tool.builtin.mech_circular_hole", "Circular Hole"),
        ],
    },
    Section {
        root: "Solids",
        title: "Voxels",
        commands: &[
            ("tool.builtin.voxel", "Voxel"),
            ("tool.builtin.voxel_volume", "Voxel Volume"),
            ("tool.builtin.voxel_frame", "Voxel Frame"),
            ("edit.voxelize_faces", "Voxelize Faces"),
            ("edit.new_voxel_group", "New Voxel Group"),
        ],
    },
    Section {
        root: "Civil",
        title: "Architecture",
        commands: &[
            ("tool.builtin.arch_wall", "Wall"),
            ("tool.builtin.arch_slab", "Rectangular Slab"),
            ("tool.arch_slab_freeform", "Freeform Slab"),
            ("tool.builtin.arch_add_hole", "Add Hole"),
            ("tool.builtin.arch_stair", "Stair"),
            ("tool.builtin.arch_window_frame", "Window"),
            ("tool.builtin.arch_door_frame", "Door"),
        ],
    },
    Section {
        root: "Civil",
        title: "Stair Inputs",
        commands: &[
            ("architecture.stair_contour", "Set Stair Contour C"),
            ("architecture.stair_path", "Set Walking Path P"),
        ],
    },
    Section {
        root: "Civil",
        title: "HVAC",
        commands: &[
            ("tool.builtin.hvac_plumbing", "HVAC Plumbing"),
            ("tool.builtin.hvac_ventilation", "HVAC Ventilation"),
        ],
    },
    Section {
        root: "Civil",
        title: "Settings",
        commands: &[
            ("view.architecture_settings", "Architecture Settings"),
            ("view.hvac_settings", "HVAC Settings"),
        ],
    },
    Section {
        root: "Materials",
        title: "Catalog",
        commands: &[
            ("view.panel.materials.toggle", "Material Manager"),
            ("material.new", "New Material"),
            ("material.edit", "Edit Material"),
            ("material.apply", "Apply Material"),
            ("material.delete", "Delete Unused Material"),
            ("tool.builtin.paint", "Paint"),
        ],
    },
    Section {
        root: "Materials",
        title: "Procedural",
        commands: &[
            ("material.new_grass", "New Grass"),
            ("material.new_foliage", "New Foliage"),
            ("material.new_water", "New Water"),
            ("material.new_brick", "New Brick"),
            ("material.new_wood", "New Wood"),
        ],
    },
    Section {
        root: "Materials",
        title: "Rendering",
        commands: &[
            ("render.raytrace", "RT"),
            ("render.global_illumination", "GI"),
            ("render.stop", "Stop"),
            ("render.save", "Save"),
            ("view.panel.shaders.toggle", "Shader Settings"),
        ],
    },
    Section {
        root: "Objects",
        title: "Prototypes, Instances & Groups",
        commands: &[
            ("view.objects", "Object Prototypes"),
            ("view.object_info", "Object Instance"),
            ("object.create_from_selection", "Make Object"),
            ("object.place_selected_definition", "Place Instance"),
            ("selection.edit_object", "Edit Prototype"),
            ("selection.exit_context", "Finish Editing"),
            ("object.explode_selection", "Explode"),
            ("object.array_selection", "Instance Array"),
            ("object.toggle_place_glue", "Glue to Surface"),
        ],
    },
    Section {
        root: "Objects",
        title: "Scripts & GPU",
        commands: &[
            ("script.new", "New Scripted Object"),
            ("script.edit", "Edit Script"),
            ("edit.copy_as_script", "Copy as Script"),
            ("object.new_tree", "New Shader Object"),
            ("object.edit_tree", "Shader Object Settings"),
            ("object.bake_to_shader", "Bake to Shader"),
            ("object.bake_to_mesh", "Bake to Mesh"),
        ],
    },
    Section {
        root: "Objects",
        title: "External Prototypes",
        commands: &[
            ("object.import_scene", "Import Prototype"),
            ("object.reference_scene", "Reference Scene"),
            ("object.refresh_reference", "Refresh Reference"),
        ],
    },
    Section {
        root: "Objects",
        title: "Hotspots",
        commands: &[
            ("hotspot.begin_placement", "Place Hotspot"),
            ("hotspot.add", "Hotspot at Cursor"),
            ("hotspot.attach", "Attach Selection"),
            ("hotspot.select_attached", "Select Attached"),
            ("hotspot.reference", "Pick Reference"),
            ("hotspot.clear_reference", "Clear Reference"),
            ("hotspot.delete", "Delete Hotspot"),
            ("view.hotspot_settings", "Hotspot Settings"),
        ],
    },
    Section {
        root: "Files",
        title: "Projects",
        commands: &[
            ("file.new", "New Drawing"),
            ("file.open", "Open…"),
            ("file.open_recent", "Recent Files"),
            ("file.save", "Save"),
            ("file.save_as", "Save As…"),
            ("file.cancel_open", "Cancel Open"),
            ("app.quit", "Exit"),
        ],
    },
    Section {
        root: "Files",
        title: "Import & Export",
        commands: &[
            ("import.mesh", "Import Mesh…"),
            ("export.mesh", "Export Mesh…"),
            ("export.ifc_model", "Export IFC…"),
            ("export.svg_view", "Export SVG…"),
            ("export.screenshot", "Save Screenshot…"),
            ("interchange.cancel", "Cancel Import / Export"),
        ],
    },
    Section {
        root: "Settings",
        title: "Model & Interface",
        commands: &[
            ("view.model_settings", "Model Settings"),
            ("view.panel.ui_settings.toggle", "UI Settings"),
            ("view.workspace.reset_layout", "Reset Layout"),
            ("view.toolbars.toggle", "Show Toolbars"),
            ("view.panels.toggle", "Show Inspectors"),
        ],
    },
    Section {
        root: "Settings",
        title: "Camera",
        commands: &[
            ("view.frame_document", "Fit Model"),
            ("view.camera.reset", "Reset Camera"),
            ("view.orbit", "Orbit"),
            ("view.camera.perspective", "Perspective"),
            ("view.camera.orthographic", "Orthographic"),
            ("view.camera.walkthrough", "Walkthrough"),
            ("view.ortho.front", "Front"),
            ("view.ortho.back", "Back"),
            ("view.ortho.left", "Left"),
            ("view.ortho.right", "Right"),
            ("view.ortho.top", "Top"),
            ("view.ortho.bottom", "Bottom"),
        ],
    },
    Section {
        root: "Settings",
        title: "Workspace Profiles",
        commands: &[
            ("view.profile.menus", "Desktop Menus"),
            ("view.profile.buttons", "Buttons"),
            ("view.profile.hybrid", "Hybrid"),
        ],
    },
    Section {
        root: "Settings",
        title: "Automation & Help",
        commands: &[
            ("view.panel.mcp.toggle", "MCP Server"),
            ("view.plugin_manager", "Plugin Manager"),
            ("view.panel.tutorials.toggle", "Tutorials"),
            ("help.about", "About Meshcraft"),
        ],
    },
];
pub(crate) fn sections(root: &str) -> impl Iterator<Item = &'static Section> {
    SECTIONS.iter().filter(move |section| section.root == root)
}
pub(crate) fn placement(id: &str) -> Option<(&'static Section, &'static str)> {
    SECTIONS.iter().find_map(|section| {
        section
            .commands
            .iter()
            .find(|(key, _)| *key == id)
            .map(|(_, label)| (section, *label))
    })
}
pub(crate) fn present(command: &mut CommandDescriptor) {
    command.presentation.menu_aliases.clear();
    if let Some((section, label)) = placement(command.id.as_str()) {
        // Old labels remain searchable after the presentation moves.
        command
            .keywords
            .push(command.presentation.palette_path.clone());
        command.label = label.into();
        command.category = section.root.into();
        command.presentation.palette_path =
            format!("{}> {}> {}", section.root, section.title, label);
        command.presentation.menu_path = vec![section.root.into(), section.title.into()];
        command.presentation.toolbar_group = Some(section.root.into());
    } else {
        command.presentation.menu_path.clear();
        command.presentation.toolbar_group = None;
    }
}

/// Preserve saved visibility flags when presenting the consolidated toolbar groups.
/// Files is new and defaults to visible. Old command IDs remain registered.
pub(crate) fn toolbar_key(root: &str) -> &'static str {
    match root {
        "2D Construction" => "construction_points",
        "Meshing" => "modification",
        "Solids" => "solids",
        "Civil" => "architecture",
        "Materials" => "rendering",
        "Objects" => "actions",
        "Files" => "files",
        _ => "camera",
    }
}

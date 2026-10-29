"""Explicit command icon designs; shared symbols represent the same operation only.

No substring guessing: e.g. stairs must never match 'arc', nor a material match
paint merely because it belongs to the materials category. Badges communicate
new/edit/delete/copy/reference actions without losing the subject's silhouette.
"""
from tool_icon_shapes import path, rect, circle, arrow, badge, BOX, GEAR, GRID, FILE, WHITE, BLUE, ORANGE, GREEN, PURPLE


def symbols():
    mesh = path('M4 24L10 4L28 10L26 29ZM10 4L16 18L4 24M16 18L28 10M16 18L26 29', GREEN)
    wall = path('M4 12L24 5L28 8V24L8 29L4 26ZM4 12L8 15L28 8M8 15V29', PURPLE)
    slab = path('M3 14L20 5L29 11L12 21ZM3 14V19L12 26L29 16V11M12 21V26', PURPLE)
    freeform = path('M3 13L13 5L20 10L27 7L29 18L19 26L12 22L5 26ZM3 13L5 26M3 17L13 9L20 14L27 11', PURPLE)
    window = rect(4,3,24,26,PURPLE)+rect(8,7,16,18,BLUE)+path('M16 7V25M8 16H24',BLUE)
    door = path('M5 29V3H27V29M9 29V7H23V29M9 7L20 11V29L9 25Z',PURPLE)+circle(17,20,1,ORANGE,ORANGE)
    stair = path('M3 28V22H9V16H15V10H21V4H29M3 28H29V4',PURPLE)
    hotspot = circle(16,16,7,BLUE)+path('M16 3V7M16 25V29M3 16H7M25 16H29',BLUE)
    material = circle(15,15,11,WHITE,'#355d76')+path('M9 8Q19 5 23 13M6 18Q16 24 25 17',BLUE)
    shader = path('M16 3L28 10V23L16 30L4 23V10ZM4 10L16 17L28 10M16 17V30',PURPLE)+path('M12 10L8 13L12 16M20 20L24 23L20 26',GREEN)
    script = rect(3,4,26,24,BLUE)+path('M12 11L7 16L12 21M20 11L25 16L20 21M18 9L14 23',GREEN)
    folder = path('M3 26V7H12L15 10H29V26ZM3 26L8 15H29L25 26Z',BLUE)
    save = path('M5 4H23L28 9V28H5ZM10 4V13H22V4M10 28V19H23V28',BLUE)+rect(18,6,2,4,WHITE,WHITE,0)
    pointer = path('M6 3L27 17L18 19L14 29Z',ORANGE,'#684828')
    line = path('M5 25L27 7',BLUE)+circle(5,25,2,ORANGE,ORANGE)+circle(27,7,2,ORANGE,ORANGE)
    polyline = path('M4 25L9 7L20 12L28 5',BLUE)+''.join(circle(x,y,1.7,ORANGE,ORANGE) for x,y in [(4,25),(9,7),(20,12),(28,5)])
    dimension = path('M5 5V28M27 5V28M5 10H27M9 6L5 10L9 14M23 6L27 10L23 14',BLUE)+path('M13 20V26M18 20V26',WHITE)
    text = path('M5 27L15 4H18L28 27M9 19H24',WHITE)
    move = arrow('M3 16H29M16 3V29M3 16L8 11M3 16L8 21M29 16L24 11M29 16L24 21M16 3L11 8M16 3L21 8M16 29L11 24M16 29L21 24',BLUE)
    rotate = path('M7 24A12 12 0 1 1 26 7M26 7V15H18',BLUE)+circle(16,16,2,ORANGE,ORANGE)
    sweep = path('M3 26H12V13H28V5H5V26M5 18H12M20 5V13',BLUE)+arrow('M20 21H29M25 17L29 21L25 25',ORANGE)
    plane = path('M2 21L23 8L30 13L9 27Z',BLUE,'#28465b')
    eye = path('M2 16Q16 -4 30 16Q16 36 2 16Z',BLUE)+circle(16,16,5,WHITE)
    cross = path('M7 7L25 25M25 7L7 25',ORANGE)
    ruler = rect(3,8,26,16,BLUE)+path('M8 8V16M13 8V13M18 8V16M23 8V13',WHITE)
    tree = path('M16 29V12M16 18L8 10M16 22L25 14',ORANGE)+path('M16 3L8 14H12L3 22H29L21 14H25Z',GREEN,'#24513c')
    return dict(
        wall=wall, slab=slab, freeform_slab=freeform, window=window, door=door, stairs=stair,
        hole=wall+rect(12,14,8,12,ORANGE,'#202730'),
        stair_contour=path('M3 28V5H28V28H20V13H11V28Z',PURPLE)+path('M7 23V9H24V23',BLUE),
        stair_path=stair+arrow('M7 27L26 7M19 7H26V14',ORANGE),
        control=wall+circle(4,26,3,ORANGE,ORANGE)+circle(28,8,3,BLUE,BLUE),
        slab_vertex=freeform+circle(13,5,3,ORANGE,ORANGE),
        architecture=wall+badge('check',GREEN), hvac_pipe=path('M4 27V13C4 7 9 4 15 4H28V12H15C12 12 12 14 12 17V27ZM4 23H12M24 4V12',GREEN),
        hvac_duct=path('M3 27V10L10 4H28V14H15V27ZM3 10H15L22 4M15 10V27M22 4V14',GREEN),
        select=pointer, selection=rect(3,4,26,24,BLUE)+path('M7 9H11M7 15H16M7 21H16',WHITE)+pointer,
        clear_selection=pointer+badge('cross'), selection_display=eye+badge('check',GREEN),
        line=line, construction_line=line+path('M2 28L3 27M12 18L14 16M21 9L23 7M29 5L30 4',ORANGE),
        point=circle(16,16,3,ORANGE,ORANGE)+path('M16 3V10M16 22V29M3 16H10M22 16H29',BLUE),
        polyline=polyline, rectangle=rect(4,6,24,20,BLUE)+rect(2,4,4,4,ORANGE,ORANGE,0)+rect(26,24,4,4,ORANGE,ORANGE,0),
        circle=circle(16,16,11,BLUE)+path('M16 16L25 8',ORANGE), arc=path('M5 25A14 14 0 0 1 27 9',BLUE)+path('M16 16L5 25M16 16L27 9',ORANGE),
        double_line=path('M3 23L11 5L27 12M7 28L15 11L30 18',BLUE),
        dimension=dimension, text=text, vector_text=text+circle(15,4,2,BLUE,BLUE)+circle(5,27,2,BLUE,BLUE)+circle(28,27,2,BLUE,BLUE),
        leader=arrow('M4 26L15 10H28M4 26L4 19M4 26L11 24',BLUE), ruler=ruler,
        axis_guide=path('M3 27L29 3M3 27L8 26M29 3L28 8M16 3V29M3 16H29',BLUE)+circle(16,16,2,ORANGE,ORANGE),
        grid_guide=GRID+circle(16,16,3,ORANGE,ORANGE), grid=GRID,
        guides=path('M3 25L25 3M7 29L29 7M3 10H29M10 3V29',BLUE),
        finish=path('M5 17L13 26L28 6',GREEN), cancel=cross,
        undo=path('M8 10H20C28 10 29 25 20 26H13M8 10L14 4M8 10L14 16',BLUE),
        redo=path('M24 10H12C4 10 3 25 12 26H19M24 10L18 4M24 10L18 16',BLUE),
        copy=rect(4,3,17,20,BLUE)+rect(12,11,16,18,WHITE,'#26313e'),
        cut=circle(7,24,4,BLUE)+circle(23,24,4,BLUE)+path('M9 21L24 3M21 21L8 3M16 13V16',WHITE),
        paste=rect(5,7,22,22,BLUE)+rect(11,3,10,7,WHITE,'#26313e')+path('M11 16H21M11 22H21',WHITE),
        paste_in_place=rect(5,7,22,22,BLUE)+rect(11,3,10,7,WHITE,'#26313e')+path('M16 13V27M9 20H23',ORANGE)+circle(16,20,3,ORANGE),
        delete=path('M7 9H25M12 9V5H20V9M9 9L10 28H22L23 9M14 13V24M18 13V24',ORANGE),
        file=FILE, folder=folder, recent=folder+circle(23,22,7,ORANGE,'#26313e')+path('M23 18V22L27 24',ORANGE), save=save,
        import_mesh=mesh+arrow('M1 8H17M12 3L17 8L12 13',ORANGE),
        export_mesh=mesh+arrow('M16 5H30M25 1L30 5L25 9',ORANGE),
        formats=FILE+path('M12 15H22M12 21H22M12 26H19',BLUE),
        ifc=wall+arrow('M16 6H30M26 2L30 6L26 10',GREEN), svg=path('M4 26L11 6L27 24ZM11 6L27 24',BLUE)+''.join(rect(x,y,4,4,ORANGE,ORANGE,0) for x,y in [(2,24),(9,4),(25,22)]),
        screenshot=rect(3,8,26,20,BLUE)+circle(16,18,6,WHITE)+path('M8 8L11 3H21L24 8',BLUE),
        exit=path('M14 5H5V27H14',WHITE)+arrow('M11 16H28M22 10L28 16L22 22',ORANGE),
        hotspot=hotspot, hotspot_attach=hotspot+path('M3 3L10 10M22 22L29 29',GREEN)+circle(3,3,2,GREEN,GREEN)+circle(29,29,2,GREEN,GREEN),
        hotspot_reference=hotspot+path('M16 16L28 4M23 4H28V9',ORANGE),
        hotspot_select=hotspot+path('M5 4L15 10L9 12Z',ORANGE,ORANGE),
        material=material, paint=path('M8 23L23 4L28 8L13 27ZM8 23C2 23 6 30 2 30C10 30 13 29 13 27',ORANGE)+path('M8 23L13 27',WHITE),
        grass=path('M3 28H29M7 28L3 9L13 25L16 3L19 25L29 10L25 28',GREEN),
        foliage=path('M16 29V13M16 22L7 14M16 18L25 10',ORANGE)+path('M7 14Q0 8 4 3Q15 4 7 14ZM25 10Q18 1 29 3Q32 13 25 10ZM16 15Q9 8 16 3Q24 8 16 15Z',GREEN,'#28523d'),
        water=path('M2 12Q6 6 11 12T21 12T31 12M2 20Q6 14 11 20T21 20T31 20M2 28Q6 22 11 28T21 28T31 28',BLUE),
        brick=rect(3,4,26,24,ORANGE)+path('M3 12H29M3 20H29M16 4V12M10 12V20M23 12V20M16 20V28',ORANGE),
        wood=rect(3,4,26,24,ORANGE)+path('M8 4Q1 15 9 28M14 4Q6 14 17 28M24 4Q32 18 24 28M17 12C11 17 18 25 22 20C27 14 20 8 17 12Z',ORANGE),
        shader=shader, script=script, tree=tree,
        parameters=path('M3 8H29M3 16H29M3 24H29',WHITE)+rect(9,4,4,8,BLUE,'#26313e',0)+rect(21,12,4,8,ORANGE,'#26313e',0)+rect(6,20,4,8,GREEN,'#26313e',0),
        raytrace=circle(22,22,6,PURPLE,'#354469')+path('M2 4L22 22L8 24M16 16L25 4M20 4H25V9',ORANGE),
        illumination=circle(16,16,5,ORANGE,'#674b30')+path('M3 3L10 10M22 22L29 29M29 3L22 10M10 22L3 29M16 2V6M16 26V30M2 16H6M26 16H30',ORANGE),
        stop=rect(6,6,20,20,ORANGE,ORANGE), mesh=mesh,
        fill_loop=path('M4 25L9 5L27 9L27 27Z',BLUE,'#315e76')+path('M9 5L27 27M4 25L27 9',GREEN)+circle(4,25,2,ORANGE,ORANGE),
        outline=path('M4 25L9 5L27 9L27 27Z',ORANGE)+path('M9 5L27 27M4 25L27 9',BLUE),
        flip=path('M3 23L12 7L21 23Z',BLUE,'#28465b')+arrow('M26 29V3M22 7L26 3L30 7M22 25L26 29L30 25',ORANGE)+arrow('M4 27H19M15 24L19 27L15 30',GREEN),
        mirror=path('M16 3V29M4 25L12 7V25ZM28 25L20 7V25Z',BLUE),
        move=move, rotate=rotate,
        rotate_two=rotate+path('M16 16H28M16 16L8 7',GREEN),
        scale=rect(4,15,13,13,WHITE)+rect(15,4,13,13,BLUE)+arrow('M11 21L25 7M18 7H25V14',ORANGE),
        stretch=path('M5 25L8 8H21L27 25ZM8 8L18 18M5 25L18 18L27 25',BLUE)+arrow('M19 5H29M25 2L29 5L25 9',ORANGE),
        squash=rect(10,7,12,18,BLUE)+arrow('M2 16H9M5 12L9 16L5 20',ORANGE)+arrow('M30 16H23M27 12L23 16L27 20',ORANGE),
        extrude=BOX+arrow('M16 19V2M11 7L16 2L21 7',ORANGE), sweep=sweep,
        sweep3d=path('M3 28H11V14L23 7L29 12L17 20V28ZM3 28V7L11 3L23 7M11 3V14',BLUE)+arrow('M22 24L29 17M24 17H29V22',ORANGE),
        revolve=path('M9 28V5H15V16H19V28ZM22 6C33 11 32 25 21 27M21 27L24 20M21 27L28 28',BLUE),
        offset=path('M4 24L11 8L25 15M8 28L15 14L29 21',BLUE)+arrow('M8 9L5 5M4 11L9 3',ORANGE),
        fillet=path('M5 27V5H27',WHITE)+path('M5 16Q5 27 16 27H28',ORANGE),
        chamfer=path('M5 27V5H27',WHITE)+path('M5 16L16 27H28',ORANGE),
        mesh_two_lines=path('M3 26L9 7L19 13L29 4M3 31L9 15L19 24L29 14M3 26V31M9 7V15M19 13V24M29 4V14M9 7L19 24M19 13L29 14',BLUE),
        remesh=mesh+path('M3 12H10M7 8L11 12L7 16M22 24H29M25 20L21 24L25 28',ORANGE),
        regularize=path('M3 27L8 7L19 13L28 5M3 27H28V5M8 7L28 27',GREEN)+arrow('M11 22H23M19 18L23 22L19 26',ORANGE),
        surface_rect=path('M3 21L20 6L29 12L12 28ZM3 21L29 12M20 6L12 28',BLUE),
        random= ''.join(circle(x,y,2,GREEN,GREEN) for x,y in [(5,6),(16,4),(27,8),(9,16),(24,19),(5,27),(17,25),(28,29)]),
        array_line=''.join(rect(3+i*10,10,6,10,BLUE,'#294a61',0) for i in range(3))+arrow('M4 27H28M24 24L28 27',ORANGE),
        array_plane=''.join(rect(3+i*10,4+j*12,6,8,BLUE,'#294a61',0) for i in range(3) for j in range(2)),
        array_space=''.join(path(f'M{x} {y}l5 -3l5 3v6l-5 3l-5 -3Z',BLUE) for x,y in [(2,7),(17,7),(2,21),(17,21)]),
        array_rotate=circle(16,16,10,ORANGE)+''.join(rect(x,y,5,5,BLUE,'#26313e',0) for x,y in [(13,1),(25,13),(13,25),(1,13)]),
        array_helix=path('M5 27C30 31 31 15 16 17C0 19 0 4 25 5M25 5L20 1M25 5L19 10',ORANGE)+rect(12,14,5,5,BLUE,'#26313e',0),
        object=BOX, object_instance=BOX+circle(16,22,3,GREEN,GREEN),
        prototypes=path('M3 8L11 4L19 8V20L11 24L3 20ZM3 8L11 12L19 8M11 12V24M15 25L23 29L30 25V13L23 9L20 11',BLUE),
        explode=path('M3 6L10 2L17 6L10 10ZM3 6V13L10 17V10M21 20L28 16L31 20L24 24ZM21 20V27L24 30V24',BLUE)+arrow('M13 18L20 11M15 11H20V16',ORANGE),
        bake_mesh=mesh+arrow('M2 6H16M11 2L16 6L11 10',ORANGE),
        bake_shader=shader+arrow('M2 6H16M11 2L16 6L11 10',ORANGE),
        glue=BOX+path('M3 27H29M12 24L16 28L20 24',GREEN),
        link=path('M14 9L18 5C26 -3 35 8 26 15L22 19M18 23L14 27C6 35 -3 24 6 17L10 13M10 22L23 9',BLUE),
        properties=rect(3,4,26,24,BLUE)+path('M7 10H17M7 16H17M7 22H17M23 8V12M23 14V18M23 20V24',WHITE),
        settings=GEAR, model_settings=BOX+badge('check',GREEN),
        ui_settings=rect(3,4,26,24,BLUE)+path('M3 10H29M12 10V28',WHITE)+badge('check',GREEN),
        panel=rect(3,4,26,24,BLUE)+rect(20,5,8,22,BLUE,'#355d76',0),
        toolbar=rect(3,4,26,24,BLUE)+path('M3 12H29M8 7H10M15 7H17M22 7H24',ORANGE),
        camera=path('M3 11H22V25H3ZM22 15L29 10V26L22 21M7 11L10 6H17L20 11',BLUE),
        orbit=path('M2 18C2 5 30 5 30 18C30 29 5 30 3 20M3 20L8 24M3 20L8 16',BLUE)+BOX,
        perspective=path('M3 10L13 4L28 8V26L13 29L3 22ZM3 10L13 16L28 8M13 16V29',BLUE)+path('M3 10L1 8M28 8L31 7',ORANGE),
        orthographic=rect(5,8,19,20,BLUE)+rect(10,3,19,20,BLUE)+path('M5 8L10 3M24 28L29 23',ORANGE),
        walkthrough=circle(16,5,3,BLUE)+path('M15 9L11 17L4 20M15 10L21 16H28M14 15L21 22L25 29M14 15L10 24L4 29',BLUE),
        fit=path('M3 11V3H11M21 3H29V11M29 21V29H21M11 29H3V21',BLUE)+BOX,
        plane_section=BOX+plane, cut_mesh=mesh+path('M2 21L26 6M4 25L30 9',ORANGE),
        union=path('M3 12L14 5L23 10L29 7V21L18 28L9 23L3 26ZM3 12L14 18L23 10M14 18V28',GREEN,'#28493c'),
        difference=path('M3 9L16 3L29 9V26L16 31L3 25ZM3 9L16 15L29 9M16 15V31',BLUE)+rect(16,7,13,13,ORANGE,'#202730',0)+path('M19 13H26',ORANGE),
        intersection=path('M3 9L15 3L26 9V23L15 29L3 23ZM3 9L15 15L26 9M15 15V29',BLUE)+path('M11 13L22 7L30 13V25L22 30L11 25ZM11 13L22 19L30 13M22 19V30',ORANGE)+path('M11 13L15 11L26 16V23L15 29L11 25Z',GREEN,'#305f44'),
        intersection_curve=mesh+path('M2 20Q10 5 17 19T30 15',ORANGE),
        cuboid=BOX, sphere=circle(16,16,12,PURPLE)+path('M16 4C4 8 4 24 16 28C28 24 28 8 16 4M4 16H28',PURPLE),
        cylinder=path('M5 8C5 2 27 2 27 8C27 14 5 14 5 8V25C5 31 27 31 27 25V8M5 23C5 29 27 29 27 23',PURPLE),
        cone=path('M4 25L16 3L28 25C26 31 6 31 4 25ZM16 3V28',PURPLE),
        pill=path('M7 13C7 0 25 0 25 13V20C25 33 7 33 7 20ZM7 13Q16 20 25 13',PURPLE),
        cog=GEAR, washer=circle(16,16,13,PURPLE)+circle(16,16,6,PURPLE), round_hole=slab+circle(16,15,5,ORANGE,'#202730'),
        screw=path('M7 3H25V9H7ZM12 9V28H20V9M11 12L21 16M11 18L21 22M11 24L20 28',PURPLE),
        voxel=GRID+BOX, voxel_frame=path('M4 9L16 3L28 9V24L16 30L4 24ZM4 9L16 15L28 9M16 15V30M4 16L16 22L28 16M10 6V27M22 6V27',PURPLE),
        voxel_volume=GRID+rect(3,3,26,26,PURPLE,'#29425b',0)+path('M3 12H29M3 21H29M12 3V29M21 3V29',PURPLE),
        mcp=rect(10,10,12,12,BLUE)+path('M16 2V10M2 16H10M22 16H30M16 22V30',GREEN)+''.join(circle(x,y,2,GREEN,GREEN) for x,y in [(16,2),(2,16),(30,16),(16,30)]),
        plugin=path('M4 5H12C9 0 23 0 20 5H28V13C21 10 21 23 28 20V28H20C23 21 9 21 12 28H4V20C11 23 11 10 4 13Z',GREEN),
        tutorial=path('M3 5H29V27H3ZM8 10L15 16L8 22M19 12H25M19 19H25',BLUE),
        help=circle(16,16,13,BLUE)+path('M12 10C12 4 24 6 21 13L16 17V20M16 25V25.2',WHITE),
        menus=rect(3,3,26,26,BLUE)+path('M3 10H29M8 7H10M15 7H17M22 7H24M5 12H17V26H5M8 16H14M8 21H14',WHITE),
        buttons=rect(3,3,26,26,BLUE)+''.join(rect(x,y,5,5,ORANGE) for x in [6,20] for y in [7,19]),
        hybrid=rect(3,3,26,26,BLUE)+path('M3 10H29M8 7H10M15 7H17M22 7H24M14 10V29',WHITE)+rect(6,15,5,5,ORANGE),
    )


# IDs are explicit. Related aliases intentionally share a subject; distinct verbs
# add a badge or specialized geometry instead of borrowing another command icon.
COMMANDS = {}


def assign(symbol, ids, overlay=None):
    for key in ids.split():
        if key in COMMANDS:
            raise ValueError(f'Duplicate icon assignment: {key}')
        COMMANDS[key] = dict(symbol=symbol, overlay=overlay)


assign('line','tool.builtin.line tool.line')
assign('construction_line','tool.builtin.construction_line')
assign('polyline','tool.builtin.polyline')
assign('rectangle','tool.builtin.rectangle tool.rectangle')
assign('circle','tool.builtin.circle')
assign('arc','tool.builtin.arc')
assign('point','tool.builtin.point')
assign('double_line','tool.builtin.double_line')
assign('mesh_two_lines','tool.builtin.ribbon_3d')
assign('mesh','tool.builtin.mesh tool.builtin.polyline_mesh')
assign('surface_rect','tool.builtin.surface_rectangle tool.builtin.quad')
assign('dimension','tool.builtin.linear_dimension')
assign('text','tool.builtin.text')
assign('vector_text','tool.builtin.vector_text')
assign('leader','tool.leader')
assign('text','annotation.update','edit')
assign('ruler','tool.distance')
assign('polyline','view.polyline_settings view.panel.polyline.toggle','settings')
assign('vector_text','view.vector_text_settings view.panel.vector_text.toggle','settings')
assign('axis_guide','view.axis_guide')
assign('grid_guide','view.grid_guide tool.builtin.planar_grid')
assign('axis_guide','tool.builtin.axial_grid')
assign('guides','view.guide.remove','delete')
assign('guides','view.guides.clear','clear')
assign('guides','view.guides.toggle','eye')
assign('grid','view.grid.toggle','eye')
assign('finish','tool.finish')
assign('cancel','tool.cancel')
assign('undo','tool.undo_point','point')
assign('copy','tool.copy_mode','check')
assign('select','tool.select tool.builtin.select')
assign('selection','selection.set view.selection view.panel.selection.toggle')
assign('clear_selection','edit.clear_selection')
assign('selection_display','selection.update_display')
assign('undo','edit.undo')
assign('redo','edit.redo')
assign('copy','edit.copy_selection')
assign('copy','edit.duplicate_selection','plus')
assign('cut','edit.cut_selection')
assign('paste','edit.paste')
assign('paste_in_place','edit.paste_in_place')
assign('script','edit.copy_as_script','copy')
assign('delete','edit.delete_selection')
assign('file','file.new','plus')
assign('folder','file.open')
assign('recent','file.open_recent')
assign('folder','file.cancel_open','cancel')
assign('save','file.save')
assign('save','file.save_as','edit')
assign('exit','app.quit')
assign('screenshot','export.screenshot')
assign('export_mesh','export.mesh')
assign('import_mesh','import.mesh')
assign('ifc','export.ifc_model')
assign('svg','export.svg_view')
assign('formats','interchange.formats')
assign('import_mesh','interchange.cancel','cancel')
assign('wall','tool.builtin.arch_wall architecture.create_wall edit.new_architecture_group')
assign('slab','tool.builtin.arch_slab architecture.create_rectangular_slab')
assign('freeform_slab','tool.arch_slab_freeform architecture.create_slab')
assign('window','tool.builtin.arch_window_frame architecture.create_frame')
assign('door','tool.builtin.arch_door_frame')
assign('hole','tool.builtin.arch_add_hole architecture.add_hole')
assign('hole','architecture.remove_hole','delete')
assign('stairs','tool.builtin.arch_stair architecture.create_stair')
assign('stair_contour','architecture.stair_contour')
assign('stair_path','architecture.stair_path')
assign('control','architecture.move_control')
assign('slab_vertex','architecture.move_slab_vertex')
assign('architecture','architecture.update','edit')
assign('architecture','architecture.settings view.architecture_settings','settings')
assign('hvac_pipe','tool.builtin.hvac_plumbing')
assign('hvac_duct','tool.builtin.hvac_ventilation')
assign('hvac_duct','view.hvac_settings view.panel.hvac.toggle','settings')
for subject in ['walls','slabs','stairs','frames']:
    assign({'walls':'wall','slabs':'slab','stairs':'stairs','frames':'window'}[subject],
           f'view.panel.architecture_{subject}.toggle','settings')
assign('hotspot','hotspot.begin_placement')
assign('hotspot','hotspot.add','plus')
assign('hotspot','hotspot.cancel_placement','cancel')
assign('hotspot_attach','hotspot.attach')
assign('hotspot_select','hotspot.select_attached')
assign('hotspot_reference','hotspot.reference')
assign('hotspot_reference','hotspot.clear_reference','cancel')
assign('hotspot','hotspot.delete','delete')
assign('hotspot','hotspot.update','edit')
assign('hotspot','hotspot.target','move')
assign('hotspot','view.hotspot_settings view.panel.hotspots.toggle','settings')
assign('material','material.new','plus')
assign('material','material.edit','edit')
assign('material','material.delete','delete')
assign('material','material.apply paint.apply_selection','check')
assign('material','paint.set_color','picker')
assign('material','material.save','save')
assign('material','view.panel.materials.toggle','list')
assign('parameters','material.parameters')
assign('shader','material.new_shader','plus')
assign('shader','material.program','save')
for subject in ['grass','foliage','water','brick','wood']:
    assign(subject,f'material.new_{subject}','plus')
assign('paint','tool.builtin.paint')
assign('raytrace','render.raytrace')
assign('illumination','render.global_illumination')
assign('stop','render.stop')
assign('screenshot','render.save','save')
assign('shader','view.panel.shaders.toggle view.lighting','settings')
assign('cut_mesh','tool.builtin.cut_holes tool.builtin.cut_holes_2','hole')
assign('cut_mesh','tool.builtin.cut_out_3')
assign('hole','edit.cut_rect_hole')
assign('plane_section','tool.builtin.cut_with_plane tool.builtin.plane_section')
assign('intersection_curve','tool.builtin.mesh_intersection')
assign('intersection','tool.builtin.object_cut','cut')
assign('outline','tool.builtin.face_outline')
assign('fill_loop','geometry.fill_loops')
assign('flip','geometry.flip_selection')
assign('mirror','tool.mirror')
assign('move','tool.builtin.move')
assign('move','tool.move_copy','copy')
assign('rotate','tool.builtin.rotate')
assign('rotate_two','tool.builtin.rotate_2')
assign('rotate','tool.rotate_copy','copy')
assign('stretch','tool.builtin.rotate_stretch','rotate')
assign('stretch','tool.builtin.stretch')
assign('stretch','tool.builtin.stretch_scale','scale')
assign('scale','tool.builtin.scale')
assign('squash','tool.squash')
assign('extrude','tool.builtin.push_pull')
assign('sweep','tool.builtin.extrude_swipe')
assign('sweep3d','tool.builtin.extrude_swipe_3d')
assign('revolve','tool.builtin.revolve')
assign('offset','tool.builtin.line_offset')
assign('regularize','tool.builtin.mesh_regularize')
assign('remesh','tool.builtin.surface_remesh')
assign('random','tool.builtin.random_offset','move')
assign('random','tool.builtin.random_surface_array','surface')
assign('random','view.random_tool_settings view.panel.random.toggle','settings')
assign('array_line','tool.builtin.copy_multiple')
assign('array_plane','tool.builtin.planar_translate_multiple')
assign('array_space','tool.builtin.volumetric_translate_multiple object.array_selection')
assign('array_rotate','tool.builtin.rotational_array tool.builtin.planar_rotate_multiple')
assign('array_rotate','tool.builtin.rotational_array_equalized','equal')
assign('array_helix','tool.builtin.helical_array tool.builtin.helicoidal_rotate_multiple')
assign('array_helix','tool.builtin.helical_array_equalized','equal')
assign('object','object.create_from_selection edit.group','plus')
assign('explode','object.explode_selection edit.ungroup')
assign('object','object.delete_definition object.delete_unused_definition','delete')
assign('object_instance','object.place_selected_definition tool.builtin.object_place','place')
assign('object','selection.edit_object','edit')
assign('object','selection.exit_context','check')
assign('glue','object.toggle_place_glue')
assign('prototypes','view.objects view.panel.objects.toggle')
assign('object_instance','view.object_info view.panel.object.toggle')
assign('import_mesh','object.import_scene','object')
assign('link','object.reference_scene','object')
assign('link','object.refresh_reference','refresh')
assign('script','script.new','plus')
assign('script','script.edit','edit')
assign('tree','object.new_tree','plus')
assign('tree','object.edit_tree','settings')
assign('shader','object.configure_procedural','settings')
assign('bake_mesh','object.bake_to_mesh')
assign('bake_shader','object.bake_to_shader')
assign('cuboid','geometry.add_cuboid tool.cuboid')
for subject in ['sphere','cone','cylinder','pill']:
    assign(subject,f'tool.builtin.primitive_{subject}')
assign('union','tool.builtin.solid_union')
assign('difference','tool.builtin.solid_subtraction')
assign('intersection','tool.builtin.solid_intersection')
assign('cog','tool.builtin.mech_cog_wheel')
assign('washer','tool.builtin.mech_round_washer')
assign('round_hole','tool.builtin.mech_circular_hole')
assign('screw','tool.builtin.mech_screw')
assign('voxel','tool.builtin.voxel')
assign('voxel','edit.new_voxel_group','plus')
assign('voxel','edit.voxelize_faces','mesh')
assign('voxel_frame','tool.builtin.voxel_frame')
assign('voxel_volume','tool.builtin.voxel_volume')
assign('model_settings','model.update_settings view.model_settings view.panel.model_settings.toggle')
assign('ui_settings','ui.update_preferences view.panel.ui_settings.toggle')
assign('save','document.snapshot','camera')
assign('cancel','edit.reset_scene_for_capture','file')
assign('fit','view.frame_document')
assign('camera','view.camera.reset','refresh')
assign('camera','view.camera.set_pose','move')
assign('orbit','view.orbit view.camera.orbit')
assign('orthographic','view.camera.orthographic')
assign('perspective','view.camera.perspective')
assign('walkthrough','view.camera.walkthrough')
for subject in ['front','back','left','right','top','bottom']:
    assign('orthographic',f'view.ortho.{subject}',subject)
assign('ui_settings','view.workspace.reset_layout','refresh')
assign('panel','view.panels.toggle','eye')
assign('toolbar','view.toolbars.toggle','eye')
assign('menus','view.profile.menus')
assign('buttons','view.profile.buttons')
assign('hybrid','view.profile.hybrid view.panel.profile.toggle')
assign('mcp','view.panel.mcp.toggle')
assign('plugin','view.plugin_manager')
assign('tutorial','view.panel.tutorials.toggle')
assign('help','help.about view.help_about view.panel.help.toggle')
assign('camera','view.capture_ui_minimal','hide')
assign('camera','view.capture_ui_restore','show')
for subject,symbol in [('annotation','text'),('modification','stretch'),('operations','rotate'),('planar_construction','polyline')]:
    assign(symbol,f'tool.cycle.{subject}','cycle')
for name,symbol in [('actions','copy'),('advanced_mesh','remesh'),('architecture','wall'),('arrays','array_plane'),('camera','camera'),('construction_entities','dimension'),('hvac','hvac_duct'),('in_tool_operators','finish'),('mech','cog'),('modification','move'),('modification_2','fill_loop'),('construction_points','line'),('primitives','sphere'),('random','random'),('rendering','raytrace'),('solids','union'),('voxel','voxel')]:
    for action in ['show','hide']:
        assign(symbol,f'view.toolbar.{name}.{action}',action)


def glyph(spec):
    base = symbols()[spec['symbol']]
    overlay = spec.get('overlay')
    if overlay is None:
        return base
    # Corner badges have a consistent opaque backing so the two silhouettes
    # stay legible at 16/24px, including against geometry in previews.
    drawings = {
        'edit':path('M21 28L23 23L27 20L30 23L26 28Z',ORANGE),
        'settings':circle(25,25,3,BLUE)+path('M25 19V21M25 29V31M19 25H21M29 25H31M21 21L22 22M28 28L29 29M21 29L22 28M28 22L29 21',BLUE),
        'copy':rect(20,20,7,8,BLUE)+rect(24,24,7,7,WHITE,'#26313e',0),
        'delete':path('M20 22H30M22 22V30H28V22M23 20H27',ORANGE),
        'clear':path('M20 21L29 30M29 21L20 30',ORANGE),
        'cancel':path('M21 21L29 29M29 21L21 29',ORANGE),
        'eye':path('M18 25Q25 16 32 25Q25 34 18 25Z',BLUE)+circle(25,25,2,WHITE),
        'list':path('M21 21H30M21 25H30M21 29H30',WHITE),
        'save':rect(20,20,11,11,BLUE)+path('M23 20V24H28V20M23 31V27H28V31',WHITE),
        'move':arrow('M19 25H31M25 19V31M19 25L22 22M19 25L22 28M31 25L28 22M31 25L28 28',ORANGE),
        'point':circle(25,25,3,ORANGE,ORANGE),
        'picker':path('M20 29L28 21L31 24L23 32ZM25 21L29 25',ORANGE),
        'rotate':path('M20 28A6 6 0 1 1 29 22M29 22V26H25',ORANGE),
        'scale':rect(20,24,6,6,WHITE)+arrow('M24 25L30 19M26 19H30V23',ORANGE),
        'hole':circle(25,25,4,ORANGE),
        'cut':path('M20 21L30 30M30 21L20 30',ORANGE),
        'surface':path('M19 26L26 20L31 24L24 30Z',BLUE,'#32566b'),
        'equal':path('M20 23H30M20 28H30',GREEN),
        'object':path('M20 22L25 19L31 22V28L25 31L20 28ZM20 22L25 25L31 22M25 25V31',BLUE),
        'place':path('M25 19V31M19 25H31',ORANGE)+circle(25,25,3,ORANGE),
        'refresh':path('M20 28A6 6 0 1 1 29 22M29 22V26H25',GREEN),
        'mesh':path('M19 29L22 20L31 25L28 31ZM22 20L28 31L19 29L31 25',GREEN),
        'camera':rect(19,22,12,8,BLUE)+circle(25,26,3,WHITE)+path('M21 22L23 19H28L30 22',BLUE),
        'file':path('M21 19H27L31 23V31H21ZM27 19V23H31',WHITE),
        'front':path('M20 21H30V30H20Z',BLUE,'#557c9c'),
        'back':path('M20 21H30V30H20ZM20 21L30 30M30 21L20 30',BLUE),
        'left':path('M20 21L30 19V29L20 31Z',BLUE,'#557c9c'),
        'right':path('M20 19L30 21V31L20 29Z',BLUE,'#557c9c'),
        'top':path('M19 24L25 20L31 24L25 28Z',BLUE,'#557c9c'),
        'bottom':path('M19 24L25 20L31 24L25 28ZM25 20V28M19 24H31',BLUE),
        'hide':path('M20 29L30 20',ORANGE),
        'show':path('M20 25L24 29L31 21',GREEN),
        'cycle':path('M20 28A6 6 0 1 1 29 22M29 22V26H25',ORANGE),
    }
    if overlay in ['plus','minus','cross','check','arrow']:
        return base+badge(overlay,GREEN if overlay in ['plus','check'] else ORANGE)
    return base+circle(25,25,7,'#26313e','#26313e')+drawings[overlay]

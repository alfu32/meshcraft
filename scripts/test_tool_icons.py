"""Regression checks for semantic command designs and transparent atlas geometry."""
import csv
import json
from pathlib import Path
import unittest
import xml.etree.ElementTree as ET

from design_tool_icons import outputs, ROOT, ASSETS
from tool_icon_glyphs import COMMANDS, glyph, symbols


class ToolIconTests(unittest.TestCase):
    def test_explicit_designs_cover_the_published_command_inventory(self):
        with (ROOT/'docs/tools/tools.csv').open() as stream:
            ids = {row['command-id'] for row in csv.DictReader(stream)}
        designs = json.loads((ASSETS/'commands.json').read_text())['commands']
        self.assertEqual(ids, set(designs))
        for key, spec in designs.items():
            self.assertEqual(spec, COMMANDS[key])
            self.assertIn(spec['symbol'], symbols())
            ET.fromstring('<svg>'+glyph(spec)+'</svg>')

    def test_different_operations_have_distinct_geometry(self):
        for group in [
            ['edit.copy_selection', 'edit.cut_selection', 'edit.paste', 'edit.paste_in_place'],
            ['tool.builtin.solid_union', 'tool.builtin.solid_subtraction', 'tool.builtin.solid_intersection', 'tool.builtin.mesh_intersection'],
            ['geometry.flip_selection', 'tool.mirror'],
            ['material.new_grass', 'material.new_foliage', 'material.new_water', 'material.new_brick', 'material.new_wood', 'material.new_shader'],
            ['tool.builtin.arch_wall', 'tool.builtin.arch_slab', 'tool.arch_slab_freeform', 'tool.builtin.arch_stair', 'tool.builtin.arch_window_frame', 'tool.builtin.arch_door_frame'],
            ['object.bake_to_mesh', 'object.bake_to_shader', 'script.new', 'script.edit'],
            ['view.ortho.'+face for face in ['front','back','top','bottom','left','right']],
            ['hotspot.add', 'hotspot.attach', 'hotspot.reference', 'hotspot.clear_reference', 'hotspot.delete'],
        ]:
            self.assertEqual(len(group), len({glyph(COMMANDS[key]) for key in group}), group)
        self.assertEqual(COMMANDS['tool.builtin.arch_stair']['symbol'], 'stairs')
        self.assertEqual(COMMANDS['architecture.create_slab']['symbol'], 'freeform_slab')
        self.assertEqual(COMMANDS['view.panel.mcp.toggle']['symbol'], 'mcp')

    def test_generation_is_deterministic_and_rejects_an_undesigned_command(self):
        designs = json.loads((ASSETS/'commands.json').read_text())['commands']
        catalog = dict(commands=[dict(id=key) for key in designs])
        first, dimensions = outputs(catalog)
        self.assertEqual(first, outputs(catalog)[0])
        self.assertEqual(first['icons.svg'], (ASSETS/'icons.svg').read_text())
        self.assertEqual(first['icons.mapping.csv'], (ASSETS/'icons.mapping.csv').read_text())
        self.assertEqual(first['commands.json'], (ASSETS/'commands.json').read_text())
        with self.assertRaisesRegex(ValueError, 'Design explicit icons'):
            outputs(dict(commands=[dict(id='new.command')]))
        atlas = ET.fromstring(first['icons.svg'])
        self.assertEqual(int(atlas.attrib['width']), dimensions[0])
        groups = atlas.findall('{http://www.w3.org/2000/svg}g')
        self.assertEqual(len(groups), len({g.attrib['id'] for g in groups}))
        # No global backdrop: every tile keeps the established transparent atlas.
        self.assertFalse(atlas.findall('{http://www.w3.org/2000/svg}rect'))
        with (ASSETS/'icons.mapping.csv').open() as stream:
            for row in csv.DictReader(stream, delimiter='|'):
                for size, suffix in [(32,''),(48,'_48'),(64,'_64')]:
                    x,y = int(row['start_x'+suffix]),int(row['start_y'+suffix])
                    self.assertEqual(int(row['end_x'+suffix])-x+1, size)
                    self.assertEqual(int(row['end_y'+suffix])-y+1, size)
                    self.assertLessEqual(x+size, dimensions[0]*size//32)
                    self.assertLessEqual(y+size, dimensions[1]*size//32)


if __name__ == '__main__':
    unittest.main()

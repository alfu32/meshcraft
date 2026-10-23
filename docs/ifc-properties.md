# IFC properties

Meshcraft uses the latest official IFC schema, **IFC 4.3.2.0 / IFC4X3_ADD2**.
The offline catalog contains all **3,988 property and quantity definitions** from
buildingSMART's official PSD archive, with 436 IFC type names, including 109 concrete `IfcValue` types. The original
schema and archive are pinned in [the vendored source](../vendor/ifc4.3.2.0/README.md).
No network service is used while editing or evaluating properties.
Browse the [complete list grouped by set](ifc4-properties.md), or download the
[searchable CSV](ifc4-properties.csv).

- **Object Prototypes** edits creation defaults on the selected prototype. Add,
  edit or delete entries; search the full standard catalog or use a custom set/name.
- **Object Instance** edits the selected instances' own property sets. A change
  applies to all selected instances in one Undo transaction, preserving other fields.
- Placing a new instance copies the current prototype defaults. Subsequent
  prototype edits affect future instances; existing instances retain their snapshot.
- Deleting a prototype default suppresses the matching script declaration until
  it is explicitly added again; existing instance values stay intact.
- Copy retains the source instance's values. Deleting an instance property does
  not reveal a prototype fallback. Save/Open preserves both scopes.
- This ownership applies equally to graphical, imported and scripted objects.

Each row opens a property editor with set, name, kind, IFC value type, value, unit
and description. Single values accept text, numbers and booleans according to the
IFC type. Use JSON for structured values:

| Kind | Example value |
| --- | --- |
| `single` | `"EI60"`, `3.5`, `true` |
| `enumerated` | `["SOLIDWALL"]` |
| `list` | `[1, 2, 3]` |
| `bounded` | `{"lower":1,"upper":5,"setPoint":3}` |
| `table` | `{"defining":[0,1],"defined":[10,20]}` |
| `reference` | `{"location":"https://example.com/spec","identification":"A","name":"Spec"}` |
| `complex` | `{"Code":{"kind":"single","dataType":"IfcLabel","value":"A"}}` |
| `quantity` | `12.5` with a catalog quantity type such as `Q_AREA` |

The editor also accepts an advanced IFC options JSON object: `secondaryDataType`
and `secondaryUnit` for the table's defined values, `expression`, `interpolation`,
`usageName` for complex properties, and `enumeration` for allowed choices. The
script API accepts the same options as its fourth argument (including `dataType`
and `kind`) or its sixth argument. For example:

```javascript
$IFC('Meshcraft_Custom', 'Curve', {defining:[0,1], defined:[10,20]}, {
  kind:'table', dataType:'IfcTimeMeasure', secondaryDataType:'IfcLengthMeasure',
  unit:'s', secondaryUnit:'m', interpolation:'LINEAR'
});
```

Empty values represent unset optional values. IFC measure values use their declared
unit; the value itself is retained, not converted by model display units. The
editor validates value structure and basic IFC scalar types; it is not a full
IFC model-view certification or relationship validator.

## Small architectural presets

The Object Prototypes and Object Instance panels provide four opt-in buttons under
“Add common properties”. They add only these starter fields:

| Preset | Property set | Fields |
| --- | --- | --- |
| Wall · 6 | `Pset_WallCommon` | Status, IsExternal, LoadBearing, FireRating, AcousticRating, ThermalTransmittance |
| Slab · 6 | `Pset_SlabCommon` | Status, IsExternal, LoadBearing, FireRating, AcousticRating, ThermalTransmittance |
| Door · 9 | `Pset_DoorCommon` | Status, IsExternal, FireRating, AcousticRating, ThermalTransmittance, HandicapAccessible, FireExit, SelfClosing, SmokeStop |
| Window · 7 | `Pset_WindowCommon` | Status, IsExternal, FireRating, AcousticRating, ThermalTransmittance, SecurityRating, GlazingAreaFraction |

Values start **Unset**, including booleans and renovation status. Existing values,
types, descriptions and units are preserved. Reapplying a preset adds only missing
fields, with no history entry when everything is already present. An application
to multiple selected instances is one Undo transaction. Prototype presets affect
future placements; current instances retain their snapshots. Explicitly reapplying
a preset restores a previously deleted field as unset.

These are Meshcraft convenience subsets of the official catalog, rather than
complete property sets or fixed Archicad/Revit export configurations. They do not
assign an IFC entity class, calculate quantities or infer fire/thermal compliance.
Use Add IFC Property for any other standard or custom field. Custom sets default
to `Meshcraft_Custom`; `Pset_`/`Qto_` names are reserved for standard sets.

## Scripts

```javascript
// Prototype creation defaults.
$IFC('Pset_WallCommon', 'FireRating', 'EI60');
$IFC('Pset_WallCommon', 'IsExternal', true, 'IfcBoolean');

function onCreate() {
  // Additional instance creation properties.
  $IFC('Meshcraft_Custom', 'SerialNumber', '001');
}

function onAnimationFrame(params, model) {
  const rating = this.ifc.Pset_WallCommon.FireRating.value;
  return this.geometry;
}
```

`$IFC(set, name, value, dataType='IfcLabel', kind='single', options={})` uses the current
execution phase, including calls through helper functions. Declare outside hooks
or in `onCreate`, never in `onAnimationFrame`. Script defaults initialize missing
creation values. Existing instance metadata stays independent across frames and
source edits. Read it through the immutable `this.ifc` map in either hook; model
query objects also expose their instance IFC metadata. `$IFC` has the same bold
magenta highlighting, completion and hover help as the other model declarations.

## MCP

`meshcraft_ifc` exposes the catalog, properties and undoable updates:

```json
{"action":"catalog","query":"Pset_WallCommon"}
{"action":"get","prototype":true}
{"action":"get"}
{"action":"set","definition_id":42,"set":"Pset_WallCommon","name":"FireRating","property":{"value":"EI60"}}
{"action":"set","definition_id":42,"paths":[[100],[101]],"set":"Pset_WallCommon","name":"FireRating","property":{"value":"EI90"}}
{"action":"delete","definition_id":42,"paths":[[100]],"set":"Pset_WallCommon","name":"FireRating"}
```

Discover/apply the same starter presets through MCP:

```json
{"action":"presets"}
{"action":"preset","definition_id":42,"preset":"wall"}
{"action":"preset","definition_id":42,"paths":[[100],[101]],"preset":"window"}
```

The apply response includes `added` and the document revision. Preset IDs are
`wall`, `slab`, `door`, `window`; the discovery response lists their fields.

Get reads the current object/prototype selection. Omit `paths` on writes for
prototype defaults; occurrence paths address instance-owned properties.

## IFC files

IFC export now identifies its schema as `IFC4X3_ADD2`. Per-instance metadata is
written as IFC property sets and quantity sets attached to each geometry proxy;
import retains supported property kinds on the imported product instances.
Units supported for explicit export are `m`, `mm`, `cm`, `m2`/`m²`, `m3`/`m³`,
`s`, `kg` and `K`; unsupported explicit units fail with a diagnostic. Property
units never change the geometry's project unit. Document references are emitted
as `IfcDocumentReference` values. Other reference metadata is preserved in project
files; exporting it requires an IFC relationship graph and fails with a diagnostic. Geometry stays a tessellated/linework proxy;
other BIM relationships and surface styles remain outside this exporter.

References: [official release database](https://technical.buildingsmart.org/standards/ifc/ifc-schema-specifications/),
[official schema documentation](https://standards.buildingsmart.org/IFC/RELEASE/IFC4_3/).

## Archicad and Revit exchange

Neither application has one immutable export-property list. The element's IFC
class, schema version, template and export configuration determine the result.

Archicad's documented predefined mappings include Structural Function →
`Pset_…Common.LoadBearing`, Position → `Pset_…Common.IsExternal`, and Renovation
Status → `AC_Pset_RenovationAndPhasing`. Element identity maps to `Name`/`Tag`;
door/window width and height map to `OverallWidth`/`OverallHeight` attributes.
Other property mappings are configured in the export translator.
[Graphisoft predefined mappings](https://help.graphisoft.com/AC/29/INT/_AC29_Help/121_IFC/121_IFC-50.htm),
[translator property mapping](https://help.graphisoft.com/AC/29/INT/_AC29_Help/121_IFC/121_IFC-38.htm).

Revit's Common Property Sets option maps existing parameters into applicable
standard sets. Its documented wall example includes `Reference`, `FireRating`,
`ThermalTransmittance`, `IsExternal`, `LoadBearing` and `ExtendToStructure`.
Additional populated shared parameters or mapping rules can extend that list.
Revit-specific sets, custom sets and base quantities have separate export options.
[Autodesk Common sets](https://autodesk.ifc-manual.com/revit/ifc-export-settings-dialog/property-sets/common-property-sets),
[base quantity export](https://www.autodesk.com/support/technical/article/caas/sfdcarticles/sfdcarticles/How-to-export-model-from-Revit-to-IFC-export-with-quantities.html).

The small element-specific presets above provide a practical Meshcraft starting
point, with the full catalog available through Add Property. They do not assert
that both applications export the same fixed fields. Identity,
classification, containment and material associations also require IFC attributes
and relationships; adding similarly named custom properties does not replace them.

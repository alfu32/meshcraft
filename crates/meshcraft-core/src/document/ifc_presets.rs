//! Opt-in architectural subsets of the official Common property sets.
use super::{Properties, Property, catalog};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub set: &'static str,
    pub fields: &'static [&'static str],
}

const BUILDING: &[&str] = &[
    "Status",
    "IsExternal",
    "LoadBearing",
    "FireRating",
    "AcousticRating",
    "ThermalTransmittance",
];

pub const ALL: &[Preset] = &[
    Preset {
        id: "wall",
        label: "Wall",
        set: "Pset_WallCommon",
        fields: BUILDING,
    },
    Preset {
        id: "slab",
        label: "Slab",
        set: "Pset_SlabCommon",
        fields: BUILDING,
    },
    Preset {
        id: "door",
        label: "Door",
        set: "Pset_DoorCommon",
        fields: &[
            "Status",
            "IsExternal",
            "FireRating",
            "AcousticRating",
            "ThermalTransmittance",
            "HandicapAccessible",
            "FireExit",
            "SelfClosing",
            "SmokeStop",
        ],
    },
    Preset {
        id: "window",
        label: "Window",
        set: "Pset_WindowCommon",
        fields: &[
            "Status",
            "IsExternal",
            "FireRating",
            "AcousticRating",
            "ThermalTransmittance",
            "SecurityRating",
            "GlazingAreaFraction",
        ],
    },
];

/// Fields are deliberately unset; a preset must never invent performance or compliance data.
pub fn properties(id: &str) -> Result<Properties, String> {
    let preset = ALL
        .iter()
        .find(|p| p.id == id)
        .ok_or("Unknown IFC preset")?;
    let mut group = std::collections::BTreeMap::new();
    for name in preset.fields {
        let template = catalog()
            .properties
            .iter()
            .find(|p| p.set == preset.set && p.name == *name)
            .ok_or_else(|| format!("IFC preset entry missing: {}/{name}", preset.set))?;
        let property = Property {
            kind: template.kind,
            data_type: template.data_type.clone(),
            enumeration: (!template.choices.is_empty()).then(|| {
                template
                    .choices
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect()
            }),
            ..Default::default()
        };
        property.validate()?;
        group.insert((*name).to_owned(), property);
    }
    Ok(std::collections::BTreeMap::from([(
        preset.set.to_owned(),
        group,
    )]))
}

/// Add only absent keys, retaining authored values, units and other metadata exactly.
pub fn merge_missing(target: &mut Properties, additions: &Properties) -> usize {
    let mut added = 0;
    for (set, fields) in additions {
        let group = target.entry(set.clone()).or_default();
        for (name, property) in fields {
            if let std::collections::btree_map::Entry::Vacant(entry) = group.entry(name.clone()) {
                entry.insert(property.clone());
                added += 1;
            }
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_are_small_valid_unset_subsets_and_preserve_authored_records() {
        for preset in ALL {
            let additions = properties(preset.id).unwrap();
            assert_eq!(preset.fields.len(), additions[preset.set].len());
            assert!(additions[preset.set].values().all(|p| p.value.is_null()));
            assert!(
                !additions[preset.set].contains_key("Reference"),
                "deprecated type reference is not a starter field"
            );
            let status = &additions[preset.set]["Status"];
            assert_eq!(super::super::Kind::Enumerated, status.kind);
            assert!(
                status
                    .enumeration
                    .as_ref()
                    .unwrap()
                    .contains(&serde_json::json!("NEW"))
            );
            let mut target = additions.clone();
            target
                .get_mut(preset.set)
                .unwrap()
                .get_mut("FireRating")
                .unwrap()
                .value = serde_json::json!("EI60");
            let original = target.clone();
            assert_eq!(0, merge_missing(&mut target, &additions));
            assert_eq!(original, target);
            super::super::validate(&target).unwrap();
        }
        assert!(properties("not-a-preset").is_err());
    }
}

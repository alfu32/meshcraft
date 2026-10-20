//! Revision-keyed, rebuildable indexes for document geometry and occurrences.
//!
//! Derived data is held outside `Document`, so it is never serialized as authored
//! state. A failed rebuild leaves the previous cache inaccessible for the new
//! revision and queries fall back to direct document scans.

use super::{
    DefinitionId, Document, GeometryBundle, GeometryBundleId, OccurrenceRef, SegmentId, TriangleId,
    VertexId,
};
use crate::geometry::{Aabb3, Vec3};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeometryDerivedIndex {
    pub bounds: Option<Aabb3>,
    pub segments_by_vertex: BTreeMap<VertexId, Vec<SegmentId>>,
    pub triangles_by_vertex: BTreeMap<VertexId, Vec<TriangleId>>,
    pub triangles_by_edge: BTreeMap<(VertexId, VertexId), Vec<TriangleId>>,
    pub neighbors_by_triangle: BTreeMap<TriangleId, Vec<TriangleId>>,
}

impl GeometryDerivedIndex {
    fn build(geometry: &GeometryBundle) -> Result<Self, String> {
        geometry.validate()?;
        let mut result = Self::default();
        let mut points = geometry
            .vertices
            .values()
            .map(|vertex| vertex.position)
            .collect::<Vec<_>>();

        for segment in geometry.segments.values() {
            for reference in segment.endpoints {
                let point = geometry
                    .vertex_position(reference)
                    .ok_or_else(|| format!("segment {} has a missing endpoint", segment.id.0))?;
                points.push(point);
                result
                    .segments_by_vertex
                    .entry(reference.id)
                    .or_default()
                    .push(segment.id);
            }
        }

        for triangle in geometry.triangles.values() {
            for reference in triangle.vertices {
                let point = geometry
                    .vertex_position(reference)
                    .ok_or_else(|| format!("triangle {} has a missing vertex", triangle.id.0))?;
                points.push(point);
                result
                    .triangles_by_vertex
                    .entry(reference.id)
                    .or_default()
                    .push(triangle.id);
            }
            for edge in [
                (triangle.vertices[0].id, triangle.vertices[1].id),
                (triangle.vertices[1].id, triangle.vertices[2].id),
                (triangle.vertices[2].id, triangle.vertices[0].id),
            ] {
                let key = ordered_edge(edge.0, edge.1);
                result
                    .triangles_by_edge
                    .entry(key)
                    .or_default()
                    .push(triangle.id);
            }
        }

        for triangles in result.triangles_by_edge.values() {
            for triangle in triangles {
                let neighbors = result.neighbors_by_triangle.entry(*triangle).or_default();
                neighbors.extend(triangles.iter().copied().filter(|other| other != triangle));
            }
        }
        for segments in result.segments_by_vertex.values_mut() {
            segments.sort_unstable();
            segments.dedup();
        }
        for triangles in result.triangles_by_vertex.values_mut() {
            triangles.sort_unstable();
            triangles.dedup();
        }
        for triangles in result.triangles_by_edge.values_mut() {
            triangles.sort_unstable();
            triangles.dedup();
        }
        for neighbors in result.neighbors_by_triangle.values_mut() {
            neighbors.sort_unstable();
            neighbors.dedup();
        }
        result.bounds = Aabb3::from_points(points);
        Ok(result)
    }
}

#[derive(Clone, Debug, Default)]
pub struct DerivedDocumentIndex {
    pub revision: u64,
    pub geometries: BTreeMap<GeometryBundleId, GeometryDerivedIndex>,
    pub occurrences_by_definition: BTreeMap<DefinitionId, Vec<OccurrenceRef>>,
    pub occurrences_by_geometry: BTreeMap<GeometryBundleId, Vec<OccurrenceRef>>,
    pub world_bounds_by_occurrence: BTreeMap<OccurrenceRef, Aabb3>,
}

#[derive(Clone, Debug, Default)]
pub struct DerivedIndexCache {
    current: Option<DerivedDocumentIndex>,
}

impl DerivedIndexCache {
    pub fn cached_revision(&self) -> Option<u64> {
        self.current.as_ref().map(|index| index.revision)
    }

    /// Rebuilds into a temporary snapshot and publishes only a complete index.
    pub fn refresh(&mut self, document: &Document) -> Result<bool, String> {
        if self.cached_revision() == Some(document.visual_revision()) {
            return Ok(false);
        }
        let index = build_index(document)?;
        self.current = Some(index);
        Ok(true)
    }

    pub fn index(&mut self, document: &Document) -> Result<&DerivedDocumentIndex, String> {
        self.refresh(document)?;
        Ok(self
            .current
            .as_ref()
            .expect("refresh publishes a complete index"))
    }

    pub fn geometry(
        &mut self,
        document: &Document,
        bundle: GeometryBundleId,
    ) -> Option<&GeometryDerivedIndex> {
        self.index(document).ok()?.geometries.get(&bundle)
    }

    /// Returns cached world bounds when current, otherwise computes them directly.
    pub fn world_bounds(
        &mut self,
        document: &Document,
        occurrence: &OccurrenceRef,
    ) -> Option<Aabb3> {
        if let Ok(index) = self.index(document) {
            if let Some(bounds) = index.world_bounds_by_occurrence.get(occurrence) {
                return Some(*bounds);
            }
            return None;
        }
        direct_world_bounds(document, occurrence)
    }

    /// Returns reverse membership for a definition, falling back to an ordered walk
    /// when the document is malformed and the snapshot cannot be rebuilt.
    pub fn definition_occurrences(
        &mut self,
        document: &Document,
        definition: DefinitionId,
    ) -> Vec<OccurrenceRef> {
        if let Ok(index) = self.index(document) {
            return index
                .occurrences_by_definition
                .get(&definition)
                .cloned()
                .unwrap_or_default();
        }
        document
            .scene_geometry_refs()
            .unwrap_or_default()
            .into_iter()
            .filter(|occurrence| {
                document
                    .resolve_occurrence(occurrence)
                    .is_some_and(|resolved| resolved.definition.id == definition)
            })
            .collect()
    }

    /// Returns incident segments, using a direct scan if the index is unavailable.
    pub fn segments_at_vertex(
        &mut self,
        document: &Document,
        bundle: GeometryBundleId,
        vertex: VertexId,
    ) -> Vec<SegmentId> {
        if let Some(index) = self.geometry(document, bundle) {
            return index
                .segments_by_vertex
                .get(&vertex)
                .cloned()
                .unwrap_or_default();
        }
        find_geometry(document, bundle)
            .map(|geometry| {
                geometry
                    .segments
                    .values()
                    .filter(|segment| {
                        segment
                            .endpoints
                            .iter()
                            .any(|endpoint| endpoint.id == vertex)
                    })
                    .map(|segment| segment.id)
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn build_index(document: &Document) -> Result<DerivedDocumentIndex, String> {
    document.validate()?;
    let mut index = DerivedDocumentIndex {
        revision: document.visual_revision(),
        ..DerivedDocumentIndex::default()
    };

    for definition in document.definitions() {
        if !definition.geometry.is_empty() {
            insert_geometry_index(&mut index, &definition.geometry)?;
        }
    }
    if let Some(root) = document.scene_root_definition()
        && !root.geometry.is_empty()
    {
        insert_geometry_index(&mut index, &root.geometry)?;
    }
    for edits in document.occurrence_edits.values() {
        if let Some(geometry) = &edits.geometry_replacement {
            insert_geometry_index(&mut index, geometry)?;
        }
    }

    for occurrence in document.scene_geometry_refs()? {
        let resolved = document
            .resolve_occurrence(&occurrence)
            .ok_or_else(|| format!("cannot resolve occurrence {:?}", occurrence.path))?;
        let geometry = document
            .effective_geometry(&occurrence)
            .ok_or_else(|| format!("missing effective geometry for {:?}", occurrence.path))?;
        index
            .occurrences_by_definition
            .entry(resolved.definition.id)
            .or_default()
            .push(occurrence.clone());
        index
            .occurrences_by_geometry
            .entry(geometry.id)
            .or_default()
            .push(occurrence.clone());
        let local_bounds = index
            .geometries
            .get(&geometry.id)
            .and_then(|data| data.bounds);
        if let Some(bounds) =
            local_bounds.and_then(|bounds| transform_bounds(bounds, resolved.world_transform))
        {
            index.world_bounds_by_occurrence.insert(occurrence, bounds);
        }
    }
    Ok(index)
}

fn insert_geometry_index(
    index: &mut DerivedDocumentIndex,
    geometry: &GeometryBundle,
) -> Result<(), String> {
    if let Some(existing) = index.geometries.get(&geometry.id) {
        let candidate = GeometryDerivedIndex::build(geometry)?;
        if existing != &candidate {
            return Err(format!("geometry bundle id {} is reused", geometry.id.0));
        }
        return Ok(());
    }
    index
        .geometries
        .insert(geometry.id, GeometryDerivedIndex::build(geometry)?);
    Ok(())
}

fn direct_world_bounds(document: &Document, occurrence: &OccurrenceRef) -> Option<Aabb3> {
    let resolved = document.resolve_occurrence(occurrence)?;
    let geometry = document.effective_geometry(occurrence)?;
    let local = GeometryDerivedIndex::build(geometry).ok()?.bounds?;
    transform_bounds(local, resolved.world_transform)
}

fn transform_bounds(bounds: Aabb3, transform: crate::geometry::Affine3) -> Option<Aabb3> {
    let [min, max] = [bounds.min, bounds.max];
    Aabb3::from_points(
        [
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(min.x, max.y, max.z),
            Vec3::new(max.x, max.y, max.z),
        ]
        .map(|point| transform.transform_point(point)),
    )
}

fn find_geometry(document: &Document, id: GeometryBundleId) -> Option<&GeometryBundle> {
    document
        .definitions
        .values()
        .map(|definition| &definition.geometry)
        .chain(
            document
                .occurrence_edits
                .values()
                .filter_map(|edit| edit.geometry_replacement.as_ref()),
        )
        .find(|geometry| geometry.id == id)
}

fn ordered_edge(first: VertexId, second: VertexId) -> (VertexId, VertexId) {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

#[cfg(test)]
mod tests {
    use super::DerivedIndexCache;
    use crate::document::{Document, InstanceEdits};
    use crate::geometry::{Affine3, Vec3};

    #[test]
    fn adjacency_bounds_and_reverse_membership_rebuild_by_revision() {
        let mut document = Document::new();
        let source = document
            .add_cuboid("source", Vec3::new(2.0, 3.0, 4.0), Vec3::ZERO)
            .unwrap();
        let definition = document.resolve_occurrence(&source).unwrap().definition.id;
        let bundle = document.effective_geometry(&source).unwrap().id;
        let copies = document
            .array_root_occurrence(&source, 1_000, Vec3::new(5.0, 0.0, 0.0))
            .unwrap();

        let mut cache = DerivedIndexCache::default();
        let initial_revision = document.visual_revision();
        let bounds = cache.world_bounds(&document, &source).unwrap();
        assert_eq!(bounds.min, Vec3::ZERO);
        assert_eq!(bounds.max, Vec3::new(2.0, 3.0, 4.0));
        assert_eq!(cache.cached_revision(), Some(initial_revision));
        assert_eq!(
            cache.definition_occurrences(&document, definition).len(),
            1_001
        );
        let first_vertex = {
            let index = cache.geometry(&document, bundle).unwrap();
            assert_eq!(index.segments_by_vertex.len(), 8);
            assert_eq!(index.triangles_by_vertex.len(), 8);
            *index.segments_by_vertex.keys().next().unwrap()
        };
        assert_eq!(
            cache
                .segments_at_vertex(&document, bundle, first_vertex)
                .len(),
            3
        );
        assert_eq!(cache.index(&document).unwrap().geometries.len(), 1);
        assert_eq!(copies.len(), 1_000);

        let revision = document.visual_revision();
        let edits = InstanceEdits {
            transform: Some(Affine3::from_translation(Vec3::new(-10.0, 1.0, 2.0))),
            ..InstanceEdits::default()
        };
        assert!(document.set_occurrence_edits(source.clone(), edits.clone()));
        assert_eq!(document.visual_revision(), revision + 1);
        assert_eq!(
            cache.world_bounds(&document, &source).unwrap().min,
            Vec3::new(-10.0, 1.0, 2.0)
        );
        let revision = document.visual_revision();
        assert!(document.set_occurrence_edits(source.clone(), edits));
        assert_eq!(document.visual_revision(), revision);
    }

    #[test]
    fn failed_rebuild_uses_direct_queries_instead_of_stale_bounds() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("invalidated", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let definition = document
            .resolve_occurrence(&occurrence)
            .unwrap()
            .definition
            .id;
        let bundle = document.effective_geometry(&occurrence).unwrap().id;
        let vertex = *document
            .effective_geometry(&occurrence)
            .unwrap()
            .vertices
            .keys()
            .next()
            .unwrap();
        let mut cache = DerivedIndexCache::default();
        assert!(cache.world_bounds(&document, &occurrence).is_some());
        let cached_revision = cache.cached_revision().unwrap();

        document
            .definitions
            .get_mut(&definition)
            .unwrap()
            .geometry
            .vertices
            .get_mut(&vertex)
            .unwrap()
            .position
            .x = f64::NAN;
        document.revision = document.revision.saturating_add(1);

        assert!(cache.world_bounds(&document, &occurrence).is_none());
        assert_eq!(cache.cached_revision(), Some(cached_revision));
        assert_eq!(cache.segments_at_vertex(&document, bundle, vertex).len(), 3);
        assert_eq!(cache.definition_occurrences(&document, definition).len(), 1);
    }
}

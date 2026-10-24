//! Revision-keyed world-space acceleration for native viewport ray queries.

use meshcraft_core::{
    document::Document,
    geometry::{Aabb3, Vec3},
    selection::{SelectedEntity, SelectionTarget},
};

const BVH_LEAF_ITEMS: usize = 8;

#[derive(Clone, Copy, Debug)]
pub(crate) enum NativeSpatialPrimitive {
    Vertex(Vec3),
    Segment([Vec3; 2]),
    Triangle([Vec3; 3]),
    Occurrence,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeSpatialItem {
    pub bounds: Aabb3,
    pub primitive: NativeSpatialPrimitive,
    pub target: SelectionTarget,
}

#[derive(Clone, Debug)]
struct BvhNode {
    bounds: Aabb3,
    left: Option<usize>,
    right: Option<usize>,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeSpatialIndex {
    revision: u64,
    items: Vec<NativeSpatialItem>,
    item_order: Vec<usize>,
    nodes: Vec<BvhNode>,
}

impl NativeSpatialIndex {
    pub(crate) fn build(document: &Document) -> Result<Self, String> {
        let mut index = Self {
            revision: document.visual_revision(),
            items: Vec::new(),
            item_order: Vec::new(),
            nodes: Vec::new(),
        };

        for occurrence in document.scene_geometry_refs()? {
            let (Some(resolved), Some(geometry)) = (
                document.resolve_occurrence(&occurrence),
                document.effective_geometry(&occurrence),
            ) else {
                continue;
            };
            let mut occurrence_bounds = document.procedural_object_world_bounds(&occurrence);
            for (id, vertex) in &geometry.vertices {
                let point = resolved.world_transform.transform_point(vertex.position);
                let Some(bounds) = point_bounds(point) else {
                    continue;
                };
                include_bounds(&mut occurrence_bounds, point);
                if vertex
                    .legacy_aliases
                    .iter()
                    .any(|a| a.source == meshcraft_core::document::architecture::GENERATED)
                {
                    continue;
                }
                index.push_item(
                    bounds,
                    NativeSpatialPrimitive::Vertex(point),
                    SelectionTarget {
                        occurrence: occurrence.clone(),
                        entity: SelectedEntity::Vertex(*id),
                    },
                );
            }
            for segment in geometry.segments.values() {
                let Some(points) = segment
                    .endpoints
                    .map(|reference| geometry.vertex_position(reference))
                    .map(|point| point.map(|point| resolved.world_transform.transform_point(point)))
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .and_then(|points| <[Vec3; 2]>::try_from(points).ok())
                else {
                    continue;
                };
                for point in points {
                    include_bounds(&mut occurrence_bounds, point);
                }
                let Some(bounds) = Aabb3::from_points(points) else {
                    continue;
                };
                index.push_item(
                    bounds,
                    NativeSpatialPrimitive::Segment(points),
                    SelectionTarget {
                        occurrence: occurrence.clone(),
                        entity: meshcraft_core::document::architecture::semantic_entity(
                            &segment.extensions,
                        )
                        .unwrap_or(SelectedEntity::Segment(segment.id)),
                    },
                );
            }
            for triangle in geometry.triangles.values() {
                let Some(points) = triangle
                    .vertices
                    .map(|reference| geometry.vertex_position(reference))
                    .map(|point| point.map(|point| resolved.world_transform.transform_point(point)))
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .and_then(|points| <[Vec3; 3]>::try_from(points).ok())
                else {
                    continue;
                };
                for point in points {
                    include_bounds(&mut occurrence_bounds, point);
                }
                let Some(bounds) = Aabb3::from_points(points) else {
                    continue;
                };
                index.push_item(
                    bounds,
                    NativeSpatialPrimitive::Triangle(points),
                    SelectionTarget {
                        occurrence: occurrence.clone(),
                        entity: meshcraft_core::document::architecture::semantic_entity(
                            &triangle.extensions,
                        )
                        .unwrap_or(SelectedEntity::Triangle(triangle.id)),
                    },
                );
            }
            if !occurrence.is_scene_root()
                && let Some(bounds) = occurrence_bounds
            {
                index.push_item(
                    bounds,
                    NativeSpatialPrimitive::Occurrence,
                    SelectionTarget {
                        occurrence: occurrence.clone(),
                        entity: SelectedEntity::Occurrence,
                    },
                );
            }
        }

        index.item_order.extend(0..index.items.len());
        if !index.items.is_empty() {
            index.build_node(0, index.item_order.len());
        }
        Ok(index)
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    #[cfg(test)]
    fn item_count(&self) -> usize {
        self.items.len()
    }

    /// Returns potential hits in document/geometry order to preserve equal-depth tie behavior.
    pub(crate) fn query_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        radius: f64,
    ) -> Vec<&NativeSpatialItem> {
        self.query_ray_with_margin(origin, direction, |_| radius)
    }

    pub(crate) fn query_ray_with_margin(
        &self,
        origin: Vec3,
        direction: Vec3,
        margin: impl Fn(Aabb3) -> f64,
    ) -> Vec<&NativeSpatialItem> {
        if self.nodes.is_empty() || !origin.is_finite() || !direction.is_finite() {
            return Vec::new();
        }
        let Some(direction) = direction.try_normalized(1.0e-12) else {
            return Vec::new();
        };

        let mut stack = vec![0];
        let mut candidate_ids = Vec::new();
        while let Some(node_id) = stack.pop() {
            let node = &self.nodes[node_id];
            let radius = margin(node.bounds);
            if !radius.is_finite()
                || radius < 0.
                || ray_aabb_distance(origin, direction, node.bounds, radius).is_none()
            {
                continue;
            }
            if let (Some(left), Some(right)) = (node.left, node.right) {
                stack.push(right);
                stack.push(left);
                continue;
            }
            for order_index in node.start..node.end {
                let item_id = self.item_order[order_index];
                if ray_aabb_distance(
                    origin,
                    direction,
                    self.items[item_id].bounds,
                    margin(self.items[item_id].bounds),
                )
                .is_some()
                {
                    candidate_ids.push(item_id);
                }
            }
        }
        candidate_ids.sort_unstable();
        candidate_ids
            .into_iter()
            .map(|item_id| &self.items[item_id])
            .collect()
    }

    /// Returns primitives whose bounds contain or approach a world-space point.
    pub(crate) fn query_point(&self, point: Vec3, radius: f64) -> Vec<&NativeSpatialItem> {
        if self.nodes.is_empty() || !point.is_finite() || !radius.is_finite() || radius < 0.0 {
            return Vec::new();
        }
        let margin = Vec3::new(radius, radius, radius);
        let Some(query) = Aabb3::from_points([point - margin, point + margin]) else {
            return Vec::new();
        };
        let mut stack = vec![0];
        let mut candidate_ids = Vec::new();
        while let Some(node_id) = stack.pop() {
            let node = &self.nodes[node_id];
            if !aabb_intersects(node.bounds, query) {
                continue;
            }
            if let (Some(left), Some(right)) = (node.left, node.right) {
                stack.push(right);
                stack.push(left);
                continue;
            }
            for order_index in node.start..node.end {
                let item_id = self.item_order[order_index];
                if aabb_intersects(self.items[item_id].bounds, query) {
                    candidate_ids.push(item_id);
                }
            }
        }
        candidate_ids.sort_unstable();
        candidate_ids
            .into_iter()
            .map(|item_id| &self.items[item_id])
            .collect()
    }

    fn push_item(
        &mut self,
        bounds: Aabb3,
        primitive: NativeSpatialPrimitive,
        target: SelectionTarget,
    ) {
        self.items.push(NativeSpatialItem {
            bounds,
            primitive,
            target,
        });
    }

    fn build_node(&mut self, start: usize, end: usize) -> usize {
        let bounds = self.item_order[start..end]
            .iter()
            .map(|item| self.items[*item].bounds)
            .reduce(union_bounds)
            .expect("BVH nodes contain at least one item");
        let node_id = self.nodes.len();
        self.nodes.push(BvhNode {
            bounds,
            left: None,
            right: None,
            start,
            end,
        });
        if end - start <= BVH_LEAF_ITEMS {
            return node_id;
        }

        let axis = largest_axis(bounds);
        self.item_order[start..end].sort_unstable_by(|left, right| {
            axis_value(center(self.items[*left].bounds), axis)
                .total_cmp(&axis_value(center(self.items[*right].bounds), axis))
                .then_with(|| left.cmp(right))
        });
        let middle = start + (end - start) / 2;
        let left = self.build_node(start, middle);
        let right = self.build_node(middle, end);
        let node = &mut self.nodes[node_id];
        node.left = Some(left);
        node.right = Some(right);
        node.start = 0;
        node.end = 0;
        node_id
    }
}

fn point_bounds(point: Vec3) -> Option<Aabb3> {
    Aabb3::from_points([point])
}

fn include_bounds(bounds: &mut Option<Aabb3>, point: Vec3) {
    if let Some(bounds) = bounds {
        bounds.include(point);
    } else {
        *bounds = point_bounds(point);
    }
}

fn union_bounds(mut left: Aabb3, right: Aabb3) -> Aabb3 {
    left.include(right.min);
    left.include(right.max);
    left
}

fn aabb_intersects(left: Aabb3, right: Aabb3) -> bool {
    left.min.x <= right.max.x
        && left.max.x >= right.min.x
        && left.min.y <= right.max.y
        && left.max.y >= right.min.y
        && left.min.z <= right.max.z
        && left.max.z >= right.min.z
}

fn center(bounds: Aabb3) -> Vec3 {
    bounds.min + (bounds.max - bounds.min) * 0.5
}

fn largest_axis(bounds: Aabb3) -> usize {
    let extent = bounds.max - bounds.min;
    if extent.x >= extent.y && extent.x >= extent.z {
        0
    } else if extent.y >= extent.z {
        1
    } else {
        2
    }
}

fn axis_value(vector: Vec3, axis: usize) -> f64 {
    match axis {
        0 => vector.x,
        1 => vector.y,
        _ => vector.z,
    }
}

fn ray_aabb_distance(origin: Vec3, direction: Vec3, bounds: Aabb3, radius: f64) -> Option<f64> {
    let margin = Vec3::new(radius, radius, radius);
    let min = bounds.min - margin;
    let max = bounds.max + margin;
    let mut near = 0.0_f64;
    let mut far = f64::INFINITY;
    for (origin, direction, min, max) in [
        (origin.x, direction.x, min.x, max.x),
        (origin.y, direction.y, min.y, max.y),
        (origin.z, direction.z, min.z, max.z),
    ] {
        if direction.abs() <= 1.0e-12 {
            if origin < min || origin > max {
                return None;
            }
            continue;
        }
        let first = (min - origin) / direction;
        let second = (max - origin) / direction;
        near = near.max(first.min(second));
        far = far.min(first.max(second));
        if far < near {
            return None;
        }
    }
    (far >= 0.0 && near.is_finite()).then_some(near.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::{NativeSpatialIndex, NativeSpatialPrimitive};
    use meshcraft_core::{document::Document, geometry::Vec3};

    fn tiled_cuboids(count: usize) -> (Document, meshcraft_core::document::OccurrenceRef) {
        let mut document = Document::new();
        let mut first = None;
        let columns = 200usize;
        for index in 0..count {
            let x = (index % columns) as f64 * 3.0;
            let z = (index / columns) as f64 * 3.0;
            let occurrence = document
                .add_cuboid(
                    format!("cube-{index}"),
                    Vec3::new(1.0, 1.0, 1.0),
                    Vec3::new(x, 0.0, z),
                )
                .expect("the generated cuboid is valid");
            first.get_or_insert(occurrence);
        }
        (
            document,
            first.expect("the test scene contains at least one object"),
        )
    }

    #[test]
    fn bvh_prunes_ray_queries_for_two_thousand_objects() {
        let (document, expected_occurrence) = tiled_cuboids(2_000);
        let index = NativeSpatialIndex::build(&document).expect("native spatial index builds");
        let candidates = index.query_ray(Vec3::new(0.5, 0.5, -5.0), Vec3::Z, 0.05);
        let point_candidates = index.query_point(Vec3::new(0.5, 0.5, 0.5), 1.0e-6);

        assert_eq!(document.visual_revision(), index.revision());
        assert!(!candidates.is_empty());
        assert!(candidates.len() * 100 < index.item_count());
        assert!(candidates.iter().any(|candidate| {
            candidate.target.occurrence == expected_occurrence
                && matches!(candidate.primitive, NativeSpatialPrimitive::Triangle(_))
        }));
        assert!(!point_candidates.is_empty());
        assert!(point_candidates.len() * 100 < index.item_count());
    }
}

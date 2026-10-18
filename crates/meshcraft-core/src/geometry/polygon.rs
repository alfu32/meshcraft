//! Temporary planar topology built from model-space segments and contours.
//!
//! Coordinates crossing into `i_overlay` or `i_triangle` are checked against
//! their documented finite `f64` range first. Polygon operations use an even-odd
//! fill rule to match the legacy drawing pipeline.

use super::{GeometryTolerance, Vec2};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule as KernelOverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use i_triangle::float::triangulatable::Triangulatable;

const MAX_KERNEL_COORDINATE: f64 = f64::from_bits(0x5f30_0000_0000_0000); // 2^500

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryDiagnosticKind {
    InvalidTolerance,
    NonFiniteCoordinate,
    CoordinateOutOfRange,
    CollapsedEdge,
    OpenLoop,
    BranchedLoop,
    AmbiguousVertex,
    DegenerateLoop,
    InvalidKernelOutput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeometryDiagnostic {
    pub kind: GeometryDiagnosticKind,
    /// Segment indices or flattened contour indices involved in the issue.
    pub source_indices: Vec<usize>,
    pub message: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoopOrderResult {
    /// Closed rings without a duplicate terminal point.
    pub loops: Vec<Vec<Vec2>>,
    pub diagnostics: Vec<GeometryDiagnostic>,
}

impl LoopOrderResult {
    pub fn is_valid(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

/// Orders unordered segments into closed simple rings. Near-coincident
/// endpoints are clustered within the scale-aware tolerance. Ambiguous vertex
/// matches and graph branches are reported instead of being guessed.
pub fn order_closed_loops(
    segments: &[super::Segment2],
    tolerance: GeometryTolerance,
) -> LoopOrderResult {
    if !tolerance.is_valid() {
        return LoopOrderResult {
            loops: Vec::new(),
            diagnostics: vec![diagnostic(
                GeometryDiagnosticKind::InvalidTolerance,
                Vec::new(),
                "Geometry tolerance must be finite and nonnegative.",
            )],
        };
    }
    if segments.is_empty() {
        return LoopOrderResult::default();
    }

    let mut scale: f64 = 0.0;
    let mut endpoints = Vec::with_capacity(segments.len() * 2);
    for (index, segment) in segments.iter().enumerate() {
        if !segment.start.is_finite() || !segment.end.is_finite() {
            return LoopOrderResult {
                loops: Vec::new(),
                diagnostics: vec![diagnostic(
                    GeometryDiagnosticKind::NonFiniteCoordinate,
                    vec![index],
                    "Segment endpoints must be finite.",
                )],
            };
        }
        scale = scale
            .max(segment.start.x.abs())
            .max(segment.start.y.abs())
            .max(segment.end.x.abs())
            .max(segment.end.y.abs());
        endpoints.push(Endpoint {
            point: segment.start,
            segment: index,
            side: 0,
        });
        endpoints.push(Endpoint {
            point: segment.end,
            segment: index,
            side: 1,
        });
    }

    let epsilon = tolerance.linear_at(scale);
    if !epsilon.is_finite() {
        return LoopOrderResult {
            loops: Vec::new(),
            diagnostics: vec![diagnostic(
                GeometryDiagnosticKind::InvalidTolerance,
                Vec::new(),
                "Geometry tolerance overflowed at the input coordinate scale.",
            )],
        };
    }
    endpoints.sort_by(|a, b| {
        a.point
            .x
            .total_cmp(&b.point.x)
            .then_with(|| a.point.y.total_cmp(&b.point.y))
            .then_with(|| a.segment.cmp(&b.segment))
            .then_with(|| a.side.cmp(&b.side))
    });

    let mut vertices: Vec<Vec2> = Vec::new();
    let mut endpoint_vertex = vec![usize::MAX; segments.len() * 2];
    for endpoint in endpoints {
        let mut match_vertex = None;
        let mut ambiguous = false;
        for (vertex_id, vertex) in vertices.iter().enumerate().rev() {
            if endpoint.point.x - vertex.x > epsilon {
                break;
            }
            if endpoint.point.distance(*vertex) <= epsilon {
                if match_vertex.is_some() {
                    ambiguous = true;
                    break;
                }
                match_vertex = Some(vertex_id);
            }
        }
        if ambiguous {
            return LoopOrderResult {
                loops: Vec::new(),
                diagnostics: vec![diagnostic(
                    GeometryDiagnosticKind::AmbiguousVertex,
                    vec![endpoint.segment],
                    "Endpoint falls within tolerance of multiple distinct vertices.",
                )],
            };
        }
        let vertex_id = match match_vertex {
            Some(vertex_id) => vertex_id,
            None => {
                vertices.push(endpoint.point);
                vertices.len() - 1
            }
        };
        endpoint_vertex[2 * endpoint.segment + endpoint.side] = vertex_id;
    }

    let mut adjacency = vec![Vec::<usize>::new(); vertices.len()];
    for (edge_id, segment) in segments.iter().enumerate() {
        let start = endpoint_vertex[2 * edge_id];
        let end = endpoint_vertex[2 * edge_id + 1];
        if start == end || segment.length() <= epsilon {
            return LoopOrderResult {
                loops: Vec::new(),
                diagnostics: vec![diagnostic(
                    GeometryDiagnosticKind::CollapsedEdge,
                    vec![edge_id],
                    "Segment endpoints collapse within the selected tolerance.",
                )],
            };
        }
        adjacency[start].push(edge_id);
        adjacency[end].push(edge_id);
    }

    if let Some(incident) = adjacency.iter().find(|incident| incident.len() > 2) {
        return LoopOrderResult {
            loops: Vec::new(),
            diagnostics: vec![diagnostic(
                GeometryDiagnosticKind::BranchedLoop,
                incident.clone(),
                "Branched vertices cannot be ordered into simple loops safely.",
            )],
        };
    }
    if let Some(incident) = adjacency.iter().find(|incident| incident.len() < 2) {
        return LoopOrderResult {
            loops: Vec::new(),
            diagnostics: vec![diagnostic(
                GeometryDiagnosticKind::OpenLoop,
                incident.clone(),
                "Every loop vertex must have exactly two incident segments.",
            )],
        };
    }

    let mut used_edges = vec![false; segments.len()];
    let mut loops = Vec::new();
    for first_edge in 0..segments.len() {
        if used_edges[first_edge] {
            continue;
        }
        let first_vertex = endpoint_vertex[2 * first_edge];
        let mut current_vertex = first_vertex;
        let mut current_edge = first_edge;
        let mut ring = Vec::new();
        loop {
            if used_edges[current_edge] {
                return LoopOrderResult {
                    loops: Vec::new(),
                    diagnostics: vec![diagnostic(
                        GeometryDiagnosticKind::BranchedLoop,
                        vec![current_edge],
                        "Segment graph revisited an edge before closing a ring.",
                    )],
                };
            }
            used_edges[current_edge] = true;
            ring.push(vertices[current_vertex]);
            let next_vertex = if endpoint_vertex[2 * current_edge] == current_vertex {
                endpoint_vertex[2 * current_edge + 1]
            } else {
                endpoint_vertex[2 * current_edge]
            };
            if next_vertex == first_vertex {
                break;
            }
            let incident = &adjacency[next_vertex];
            current_edge = if incident[0] == current_edge {
                incident[1]
            } else {
                incident[0]
            };
            current_vertex = next_vertex;
        }
        if ring.len() < 3 || !ring_has_area(&ring, epsilon) {
            return LoopOrderResult {
                loops: Vec::new(),
                diagnostics: vec![diagnostic(
                    GeometryDiagnosticKind::DegenerateLoop,
                    Vec::new(),
                    "Closed loop has fewer than three vertices or negligible area.",
                )],
            };
        }
        loops.push(ring);
    }

    LoopOrderResult {
        loops,
        diagnostics: Vec::new(),
    }
}

fn ring_has_area(points: &[Vec2], epsilon: f64) -> bool {
    let origin = points[0];
    let mut twice_area = 0.0;
    let mut perimeter = 0.0;
    for index in 0..points.len() {
        let current = points[index] - origin;
        let next = points[(index + 1) % points.len()] - origin;
        twice_area += current.cross(next);
        perimeter += points[index].distance(points[(index + 1) % points.len()]);
    }
    twice_area.is_finite() && perimeter.is_finite() && twice_area.abs() > epsilon * perimeter
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Endpoint {
    point: Vec2,
    segment: usize,
    side: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PolygonShape {
    /// Outer contour followed by zero or more hole contours. The even-odd fill
    /// rule determines interior and hole semantics regardless of winding.
    pub contours: Vec<Vec<Vec2>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PolygonSet {
    pub shapes: Vec<PolygonShape>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolygonOperation {
    Union,
    Intersection,
    Difference,
    Xor,
}

impl From<PolygonOperation> for KernelOverlayRule {
    fn from(value: PolygonOperation) -> Self {
        match value {
            PolygonOperation::Union => Self::Union,
            PolygonOperation::Intersection => Self::Intersect,
            PolygonOperation::Difference => Self::Difference,
            PolygonOperation::Xor => Self::Xor,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriangulatedPolygon {
    pub points: Vec<Vec2>,
    /// Counter-clockwise point triples.
    pub triangles: Vec<[u32; 3]>,
}

impl PolygonSet {
    /// Resolves contour self-intersections with an even-odd fill rule.
    pub fn polygonize(&self) -> Result<Self, GeometryDiagnostic> {
        let subject = self.to_kernel_shapes()?;
        let empty: Vec<Vec<Vec<[f64; 2]>>> = Vec::new();
        let result = subject.overlay(&empty, KernelOverlayRule::Subject, FillRule::EvenOdd);
        Self::from_kernel_shapes(result)
    }

    /// Applies a boolean operation to two multipolygons using even-odd fill.
    pub fn overlay(
        &self,
        clip: &Self,
        operation: PolygonOperation,
    ) -> Result<Self, GeometryDiagnostic> {
        let subject = self.to_kernel_shapes()?;
        let clip = clip.to_kernel_shapes()?;
        let result = subject.overlay(&clip, operation.into(), FillRule::EvenOdd);
        Self::from_kernel_shapes(result)
    }

    /// Triangulates every polygon, including its hole contours.
    pub fn triangulate(&self) -> Result<TriangulatedPolygon, GeometryDiagnostic> {
        // i_triangle's float adapter uses non-zero winding. Resolve with the
        // app's even-odd rule first so contour direction cannot fill a hole.
        let resolved = self.polygonize()?;
        let shapes = resolved.to_kernel_shapes()?;
        let mut output = TriangulatedPolygon::default();
        for shape in shapes {
            let triangulation = shape.triangulate().to_triangulation::<u32>();
            if triangulation.indices.len() % 3 != 0
                || triangulation.points.iter().any(|point| {
                    !point[0].is_finite()
                        || !point[1].is_finite()
                        || point[0].abs() > MAX_KERNEL_COORDINATE
                        || point[1].abs() > MAX_KERNEL_COORDINATE
                })
            {
                return Err(diagnostic(
                    GeometryDiagnosticKind::InvalidKernelOutput,
                    Vec::new(),
                    "Triangulation kernel returned invalid points or index data.",
                ));
            }
            let offset = u32::try_from(output.points.len()).map_err(|_| {
                diagnostic(
                    GeometryDiagnosticKind::InvalidKernelOutput,
                    Vec::new(),
                    "Triangulation exceeded the supported u32 point index range.",
                )
            })?;
            for indices in triangulation.indices.as_chunks::<3>().0 {
                if indices
                    .iter()
                    .any(|index| *index as usize >= triangulation.points.len())
                {
                    return Err(diagnostic(
                        GeometryDiagnosticKind::InvalidKernelOutput,
                        Vec::new(),
                        "Triangulation kernel returned an out-of-range point index.",
                    ));
                }
                output.triangles.push([
                    indices[0].checked_add(offset).ok_or_else(|| {
                        diagnostic(
                            GeometryDiagnosticKind::InvalidKernelOutput,
                            Vec::new(),
                            "Triangulation point index overflowed u32.",
                        )
                    })?,
                    indices[1].checked_add(offset).ok_or_else(|| {
                        diagnostic(
                            GeometryDiagnosticKind::InvalidKernelOutput,
                            Vec::new(),
                            "Triangulation point index overflowed u32.",
                        )
                    })?,
                    indices[2].checked_add(offset).ok_or_else(|| {
                        diagnostic(
                            GeometryDiagnosticKind::InvalidKernelOutput,
                            Vec::new(),
                            "Triangulation point index overflowed u32.",
                        )
                    })?,
                ]);
            }
            output.points.extend(
                triangulation
                    .points
                    .into_iter()
                    .map(|point| Vec2::new(point[0], point[1])),
            );
        }
        Ok(output)
    }

    fn to_kernel_shapes(&self) -> Result<Vec<Vec<Vec<[f64; 2]>>>, GeometryDiagnostic> {
        let mut result = Vec::with_capacity(self.shapes.len());
        for (shape_index, shape) in self.shapes.iter().enumerate() {
            if shape.contours.iter().any(|contour| contour.len() < 3) {
                return Err(diagnostic(
                    GeometryDiagnosticKind::DegenerateLoop,
                    vec![shape_index],
                    "Polygon contours must contain at least three points.",
                ));
            }
            let mut contours = Vec::with_capacity(shape.contours.len());
            for (contour_index, contour) in shape.contours.iter().enumerate() {
                let mut points = Vec::with_capacity(contour.len());
                for point in contour {
                    if !point.is_finite() {
                        return Err(diagnostic(
                            GeometryDiagnosticKind::NonFiniteCoordinate,
                            vec![shape_index, contour_index],
                            "Polygon coordinates must be finite.",
                        ));
                    }
                    if point.x.abs() > MAX_KERNEL_COORDINATE
                        || point.y.abs() > MAX_KERNEL_COORDINATE
                    {
                        return Err(diagnostic(
                            GeometryDiagnosticKind::CoordinateOutOfRange,
                            vec![shape_index, contour_index],
                            "Polygon coordinates exceed the kernels' documented 2^500 f64 limit.",
                        ));
                    }
                    points.push([point.x, point.y]);
                }
                contours.push(points);
            }
            result.push(contours);
        }
        Ok(result)
    }

    fn from_kernel_shapes(shapes: Vec<Vec<Vec<[f64; 2]>>>) -> Result<Self, GeometryDiagnostic> {
        let mut result = Vec::with_capacity(shapes.len());
        for (shape_index, shape) in shapes.into_iter().enumerate() {
            let mut contours = Vec::with_capacity(shape.len());
            for (contour_index, contour) in shape.into_iter().enumerate() {
                if contour.len() < 3
                    || contour.iter().any(|point| {
                        !point[0].is_finite()
                            || !point[1].is_finite()
                            || point[0].abs() > MAX_KERNEL_COORDINATE
                            || point[1].abs() > MAX_KERNEL_COORDINATE
                    })
                {
                    return Err(diagnostic(
                        GeometryDiagnosticKind::InvalidKernelOutput,
                        vec![shape_index, contour_index],
                        "Polygon kernel returned an invalid contour.",
                    ));
                }
                contours.push(
                    contour
                        .into_iter()
                        .map(|point| Vec2::new(point[0], point[1]))
                        .collect(),
                );
            }
            result.push(PolygonShape { contours });
        }
        Ok(Self { shapes: result })
    }
}

fn diagnostic(
    kind: GeometryDiagnosticKind,
    source_indices: Vec<usize>,
    message: &'static str,
) -> GeometryDiagnostic {
    GeometryDiagnostic {
        kind,
        source_indices,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Segment2;

    fn point(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> PolygonSet {
        PolygonSet {
            shapes: vec![PolygonShape {
                contours: vec![vec![
                    point(x0, y0),
                    point(x1, y0),
                    point(x1, y1),
                    point(x0, y1),
                ]],
            }],
        }
    }

    fn mesh_area(set: &PolygonSet) -> f64 {
        let mesh = set.triangulate().unwrap();
        mesh.triangles
            .iter()
            .map(|triangle| {
                let a = mesh.points[triangle[0] as usize];
                let b = mesh.points[triangle[1] as usize];
                let c = mesh.points[triangle[2] as usize];
                (b - a).cross(c - a).abs() * 0.5
            })
            .sum()
    }

    #[test]
    fn orders_unordered_loop_segments_and_reports_open_graphs() {
        let segments = [
            Segment2::new(point(1.0, 1.0), point(0.0, 1.0)),
            Segment2::new(point(0.0, 0.0), point(1.0, 0.0)),
            Segment2::new(point(0.0, 1.0), point(0.0, 0.0)),
            Segment2::new(point(1.0, 0.0), point(1.0, 1.0)),
        ];
        let ordered = order_closed_loops(&segments, GeometryTolerance::default());
        assert!(ordered.is_valid());
        assert_eq!(ordered.loops.len(), 1);
        assert_eq!(ordered.loops[0].len(), 4);

        let open = order_closed_loops(&segments[..3], GeometryTolerance::default());
        assert_eq!(open.diagnostics[0].kind, GeometryDiagnosticKind::OpenLoop);
    }

    #[test]
    fn reports_branched_and_ambiguous_endpoint_graphs() {
        let branch = [
            Segment2::new(point(-1.0, 0.0), point(0.0, 0.0)),
            Segment2::new(point(0.0, 0.0), point(1.0, 0.0)),
            Segment2::new(point(0.0, 0.0), point(0.0, 1.0)),
        ];
        assert_eq!(
            order_closed_loops(&branch, GeometryTolerance::default()).diagnostics[0].kind,
            GeometryDiagnosticKind::BranchedLoop
        );

        let tolerance = GeometryTolerance::new(0.1, 0.0).unwrap();
        let ambiguous = [
            Segment2::new(point(0.0, 0.0), point(3.0, 0.0)),
            Segment2::new(point(0.0, 0.15), point(3.0, 1.0)),
            Segment2::new(point(0.05, 0.075), point(3.0, -1.0)),
        ];
        assert_eq!(
            order_closed_loops(&ambiguous, tolerance).diagnostics[0].kind,
            GeometryDiagnosticKind::AmbiguousVertex
        );
    }

    #[test]
    fn polygonizes_bow_tie_and_triangulates_holes() {
        let bow_tie = PolygonSet {
            shapes: vec![PolygonShape {
                contours: vec![vec![
                    point(0.0, 0.0),
                    point(2.0, 2.0),
                    point(0.0, 2.0),
                    point(2.0, 0.0),
                ]],
            }],
        };
        let polygonized = bow_tie.polygonize().unwrap();
        assert_eq!(polygonized.shapes.len(), 2);

        let polygon_with_hole = PolygonSet {
            shapes: vec![PolygonShape {
                contours: vec![
                    vec![
                        point(0.0, 0.0),
                        point(4.0, 0.0),
                        point(4.0, 4.0),
                        point(0.0, 4.0),
                    ],
                    vec![
                        point(1.0, 1.0),
                        point(3.0, 1.0),
                        point(3.0, 3.0),
                        point(1.0, 3.0),
                    ],
                ],
            }],
        };
        let triangulation = polygon_with_hole.triangulate().unwrap();
        assert_eq!(triangulation.triangles.len(), 8);
        let area: f64 = triangulation
            .triangles
            .iter()
            .map(|triangle| {
                let a = triangulation.points[triangle[0] as usize];
                let b = triangulation.points[triangle[1] as usize];
                let c = triangulation.points[triangle[2] as usize];
                (b - a).cross(c - a).abs() * 0.5
            })
            .sum();
        assert!((area - 12.0).abs() < 1.0e-8);

        let union = square(0.0, 0.0, 2.0, 2.0)
            .overlay(&square(1.0, 0.0, 3.0, 2.0), PolygonOperation::Union)
            .unwrap();
        assert_eq!(union.shapes.len(), 1);
        assert_eq!(union.shapes[0].contours.len(), 1);
        assert!((mesh_area(&union) - 6.0).abs() < 1.0e-8);

        let left = square(0.0, 0.0, 2.0, 2.0);
        let right = square(1.0, 0.0, 3.0, 2.0);
        assert!(
            (mesh_area(
                &left
                    .overlay(&right, PolygonOperation::Intersection)
                    .unwrap()
            ) - 2.0)
                .abs()
                < 1.0e-8
        );
        assert!(
            (mesh_area(&left.overlay(&right, PolygonOperation::Difference).unwrap()) - 2.0).abs()
                < 1.0e-8
        );
        assert!(
            (mesh_area(&left.overlay(&right, PolygonOperation::Xor).unwrap()) - 4.0).abs() < 1.0e-8
        );
    }

    #[test]
    fn rejects_invalid_kernel_coordinates_before_calling_dependencies() {
        let invalid = PolygonSet {
            shapes: vec![PolygonShape {
                contours: vec![vec![
                    point(0.0, 0.0),
                    point(MAX_KERNEL_COORDINATE * 2.0, 0.0),
                    point(0.0, 1.0),
                ]],
            }],
        };
        assert_eq!(
            invalid.polygonize().unwrap_err().kind,
            GeometryDiagnosticKind::CoordinateOutOfRange
        );
        assert_eq!(
            square(f64::NAN, 0.0, 1.0, 1.0)
                .triangulate()
                .unwrap_err()
                .kind,
            GeometryDiagnosticKind::NonFiniteCoordinate
        );
    }
}

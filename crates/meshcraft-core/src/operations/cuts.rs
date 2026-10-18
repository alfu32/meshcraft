//! Selection-based planar face cuts.
//!
//! The three legacy actions deliberately keep different semantics: `Holes` removes the area
//! inside closed selected-edge loops, `Holes2` partitions faces by finite edge loops/chords, and
//! `Cutout` partitions faces by the infinite planes implied by each selected segment.

use crate::document::Rgba8;
use crate::geometry::polygon::{PolygonOperation, PolygonSet, PolygonShape};
use crate::geometry::{GeometryTolerance, Segment2, Triangle3, Vec2, Vec3};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CutTool {
    Holes,
    Holes2,
    Cutout,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CutSourceTriangle {
    /// Caller-owned index used to remap generated faces and selection.
    pub source: usize,
    pub vertices: [Vec3; 3],
    pub color: Rgba8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CutSegment3 {
    pub start: Vec3,
    pub end: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CutOutputTriangle {
    pub source: usize,
    pub vertices: [Vec3; 3],
    pub color: Rgba8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CutResult {
    /// Replacement faces grouped by their selected source face.
    pub triangles: Vec<CutOutputTriangle>,
    /// Sources that changed; sources omitted here retain their original identity.
    pub replaced_sources: BTreeSet<usize>,
    /// Sources fully removed by `Holes`.
    pub removed_sources: BTreeSet<usize>,
}

#[derive(Clone, Copy, Debug)]
struct PlaneFrame {
    origin: Vec3,
    u: Vec3,
    v: Vec3,
    normal: Vec3,
}

impl PlaneFrame {
    fn from_triangle(face: CutSourceTriangle, tolerance: GeometryTolerance) -> Option<Self> {
        let normal = Triangle3::new(face.vertices[0], face.vertices[1], face.vertices[2])
            .normal(tolerance)?;
        let reference = if normal.x.abs() < 0.7 {
            Vec3::X
        } else {
            Vec3::Y
        };
        let mut u = reference.cross(normal).try_normalized(1.0e-14)?;
        if u.length() <= 1.0e-14 {
            u = Vec3::Z.cross(normal).try_normalized(1.0e-14)?;
        }
        let v = normal.cross(u);
        Some(Self {
            origin: face.vertices[0],
            u,
            v,
            normal,
        })
    }

    fn project(self, point: Vec3) -> Vec2 {
        let relative = point - self.origin;
        Vec2::new(relative.dot(self.u), relative.dot(self.v))
    }

    fn unproject(self, point: Vec2) -> Vec3 {
        self.origin + self.u * point.x + self.v * point.y
    }

    fn plane_distance(self, point: Vec3) -> f64 {
        (point - self.origin).dot(self.normal).abs()
    }
}

/// Computes replacement faces without mutating the document. Callers can commit the result in a
/// document transaction; any error leaves geometry and selection untouched.
pub fn cut_selected_faces(
    faces: &[CutSourceTriangle],
    segments: &[CutSegment3],
    tool: CutTool,
    tolerance: GeometryTolerance,
) -> Result<CutResult, &'static str> {
    if faces.is_empty() {
        return Err("Select faces and edges before cutting.");
    }
    if segments.is_empty() {
        return Err("Select faces and edges before cutting.");
    }
    if !tolerance.is_valid() || tolerance.absolute == 0.0 && tolerance.relative == 0.0 {
        return Err("Cut tolerance must be positive and finite.");
    }
    if segments
        .iter()
        .any(|segment| !segment.start.is_finite() || !segment.end.is_finite())
    {
        return Err("Cut segments must contain finite points.");
    }
    let mut sources = BTreeSet::new();
    if faces.iter().any(|face| {
        !sources.insert(face.source) || face.vertices.iter().any(|point| !point.is_finite())
    }) {
        return Err("Selected faces must be unique and contain finite points.");
    }

    let scale = faces
        .iter()
        .map(|face| {
            let [a, b, c] = face.vertices;
            (b - a).length().max((c - b).length()).max((a - c).length())
        })
        .chain(
            segments
                .iter()
                .map(|segment| (segment.end - segment.start).length()),
        )
        .fold(0.0, f64::max);
    let epsilon = tolerance.linear_at(scale);
    if !epsilon.is_finite() || epsilon <= 0.0 {
        return Err("Cut tolerance exceeds the supported coordinate range.");
    }

    let average_normal = (tool == CutTool::Cutout).then(|| average_normal(faces));
    if average_normal.is_some_and(|normal| normal.try_normalized(1.0e-14).is_none()) {
        return Err("Cutout needs selected faces with a consistent average normal.");
    }

    let mut result = CutResult::default();
    for face in faces {
        let frame = PlaneFrame::from_triangle(*face, tolerance)
            .ok_or("Selected faces must be non-degenerate triangles.")?;
        let triangle = face.vertices.map(|point| frame.project(point));
        let mut regions = vec![triangle_region(triangle)];
        let changed = match tool {
            CutTool::Holes | CutTool::Holes2 => {
                let projected = segments
                    .iter()
                    .filter(|segment| {
                        frame.plane_distance(segment.start) <= epsilon
                            && frame.plane_distance(segment.end) <= epsilon
                    })
                    .map(|segment| Segment2 {
                        start: frame.project(segment.start),
                        end: frame.project(segment.end),
                    })
                    .collect::<Vec<_>>();
                if projected.is_empty() {
                    false
                } else {
                    let (loops, open_segments) = order_cut_paths(&projected, epsilon)?;
                    let mut did_change = false;
                    if tool == CutTool::Holes {
                        if !loops.is_empty() {
                            let mut cutters = PolygonSet::default();
                            for ring in &loops {
                                let loop_region = PolygonSet {
                                    shapes: vec![loop_shape(ring)],
                                };
                                cutters = if cutters.shapes.is_empty() {
                                    loop_region
                                } else {
                                    cutters
                                        .overlay(&loop_region, PolygonOperation::Union)
                                        .map_err(
                                            |_| "Selected cut loops could not be resolved safely.",
                                        )?
                                };
                            }
                            let clipped = regions[0]
                                .overlay(&cutters, PolygonOperation::Difference)
                                .map_err(|_| "Selected cut loops could not be resolved safely.")?;
                            let source_area = polygon_area(&regions[0])?;
                            let kept_area = polygon_area(&clipped)?;
                            if source_area - kept_area > epsilon * epsilon {
                                regions = nonempty_regions(clipped);
                                did_change = true;
                                if regions.is_empty() {
                                    result.removed_sources.insert(face.source);
                                }
                            }
                        }
                    } else {
                        for ring in &loops {
                            let cutter = PolygonSet {
                                shapes: vec![loop_shape(ring)],
                            };
                            let mut next_regions = Vec::new();
                            for region in regions {
                                let inside = region
                                    .overlay(&cutter, PolygonOperation::Intersection)
                                    .map_err(
                                        |_| "Selected cut loops could not be resolved safely.",
                                    )?;
                                let outside = region
                                    .overlay(&cutter, PolygonOperation::Difference)
                                    .map_err(
                                        |_| "Selected cut loops could not be resolved safely.",
                                    )?;
                                if !inside.shapes.is_empty() && !outside.shapes.is_empty() {
                                    did_change = true;
                                }
                                next_regions.extend(nonempty_regions(outside));
                                next_regions.extend(nonempty_regions(inside));
                            }
                            regions = next_regions;
                        }
                    }
                    if tool == CutTool::Holes {
                        for segment in open_segments {
                            if clip_segment_to_triangle(segment, triangle, epsilon).is_none() {
                                continue;
                            }
                            // The original CUT_HOLES path partitions by the supporting line
                            // after checking that the selected segment crosses the face.
                            let mut next_regions = Vec::new();
                            for region in regions {
                                let (positive, negative) = split_region_by_line(
                                    &region,
                                    segment.start,
                                    segment.end - segment.start,
                                    triangle,
                                    epsilon,
                                )?;
                                if !positive.shapes.is_empty() && !negative.shapes.is_empty() {
                                    did_change = true;
                                    next_regions.extend(nonempty_regions(positive));
                                    next_regions.extend(nonempty_regions(negative));
                                } else {
                                    next_regions.push(region);
                                }
                            }
                            regions = next_regions;
                        }
                    } else {
                        let clipped = open_segments
                            .into_iter()
                            .filter_map(|segment| {
                                clip_segment_to_triangle(segment, triangle, epsilon)
                            })
                            .map(|(start, end)| Segment2 { start, end })
                            .collect::<Vec<_>>();
                        for path in order_open_paths(&clipped, epsilon)? {
                            if !on_triangle_boundary(path[0], triangle, epsilon)
                                || !on_triangle_boundary(
                                    *path.last().expect("ordered paths contain endpoints"),
                                    triangle,
                                    epsilon,
                                )
                            {
                                continue;
                            }
                            let Some(cutter) = boundary_path_region(&path, triangle, epsilon)?
                            else {
                                continue;
                            };
                            let mut next_regions = Vec::new();
                            for region in regions {
                                let inside = region
                                    .overlay(&cutter, PolygonOperation::Intersection)
                                    .map_err(
                                        |_| "Selected cut path could not be resolved safely.",
                                    )?;
                                let outside = region
                                    .overlay(&cutter, PolygonOperation::Difference)
                                    .map_err(
                                        |_| "Selected cut path could not be resolved safely.",
                                    )?;
                                if !inside.shapes.is_empty() && !outside.shapes.is_empty() {
                                    did_change = true;
                                }
                                next_regions.extend(nonempty_regions(outside));
                                next_regions.extend(nonempty_regions(inside));
                            }
                            regions = next_regions;
                        }
                    }
                    did_change
                }
            }
            CutTool::Cutout => {
                let normal = average_normal.expect("Cutout computes the selected-set normal");
                let mut did_change = false;
                for segment in segments {
                    let direction = segment.end - segment.start;
                    let Some(direction) = direction.try_normalized(epsilon) else {
                        continue;
                    };
                    let plane_normal = direction.cross(normal);
                    let Some(plane_normal) = plane_normal.try_normalized(1.0e-14) else {
                        continue;
                    };
                    let a = plane_normal.dot(frame.u);
                    let b = plane_normal.dot(frame.v);
                    let c = (frame.origin - segment.start).dot(plane_normal);
                    let norm2 = a * a + b * b;
                    if norm2 <= 1.0e-28 {
                        continue;
                    }
                    let point = Vec2::new(-c * a / norm2, -c * b / norm2);
                    let line = Vec2::new(-b, a);
                    let mut next_regions = Vec::new();
                    for region in regions {
                        let (positive, negative) =
                            split_region_by_line(&region, point, line, triangle, epsilon)?;
                        if !positive.shapes.is_empty() && !negative.shapes.is_empty() {
                            did_change = true;
                            next_regions.extend(nonempty_regions(positive));
                            next_regions.extend(nonempty_regions(negative));
                        } else {
                            next_regions.push(region);
                        }
                    }
                    regions = next_regions;
                }
                did_change
            }
        };

        if !changed {
            continue;
        }
        result.replaced_sources.insert(face.source);
        if !result.removed_sources.contains(&face.source) {
            for region in regions {
                let expected_area = polygon_area(&region)?;
                let triangulation = region
                    .triangulate()
                    .map_err(|_| "Cut result could not be triangulated safely.")?;
                let mut produced_area = 0.0;
                for indices in triangulation.triangles {
                    let points = indices.map(|index| triangulation.points[index as usize]);
                    let vertices = points.map(|point| frame.unproject(point));
                    let Some(area) =
                        Triangle3::new(vertices[0], vertices[1], vertices[2]).area(tolerance)
                    else {
                        continue;
                    };
                    if area <= epsilon * epsilon {
                        continue;
                    }
                    produced_area += area;
                    result.triangles.push(CutOutputTriangle {
                        source: face.source,
                        vertices,
                        color: face.color,
                    });
                }
                let area_tolerance =
                    (epsilon * expected_area.sqrt() * 8.0).max(expected_area * 1.0e-9);
                if (produced_area - expected_area).abs() > area_tolerance {
                    return Err("Cut triangulation did not preserve the expected face area.");
                }
            }
        }
    }

    if result.replaced_sources.is_empty() {
        return Err("Selected cut edges do not divide the selected faces.");
    }
    Ok(result)
}

fn average_normal(faces: &[CutSourceTriangle]) -> Vec3 {
    faces.iter().fold(Vec3::ZERO, |sum, face| {
        sum + (face.vertices[1] - face.vertices[0]).cross(face.vertices[2] - face.vertices[0])
    })
}

fn triangle_region(triangle: [Vec2; 3]) -> PolygonSet {
    PolygonSet {
        shapes: vec![PolygonShape {
            contours: vec![triangle.to_vec()],
        }],
    }
}

fn loop_shape(points: &[Vec2]) -> PolygonShape {
    PolygonShape {
        contours: vec![points.to_vec()],
    }
}

fn nonempty_regions(region: PolygonSet) -> Vec<PolygonSet> {
    region
        .shapes
        .into_iter()
        .map(|shape| PolygonSet {
            shapes: vec![shape],
        })
        .collect()
}

fn polygon_area(region: &PolygonSet) -> Result<f64, &'static str> {
    let triangulation = region
        .triangulate()
        .map_err(|_| "Cut polygon area could not be computed safely.")?;
    let area = triangulation
        .triangles
        .iter()
        .map(|indices| {
            let a = triangulation.points[indices[0] as usize];
            let b = triangulation.points[indices[1] as usize];
            let c = triangulation.points[indices[2] as usize];
            ((b - a).cross(c - a).abs()) * 0.5
        })
        .sum::<f64>();
    area.is_finite()
        .then_some(area)
        .ok_or("Cut polygon area exceeds the supported range.")
}

/// Forms simple loops from degree-two endpoint components and returns remaining open segments.
/// Branches, endpoint ambiguity and touching/self-intersecting loops fail before mutation.
fn order_cut_paths(
    segments: &[Segment2],
    epsilon: f64,
) -> Result<(Vec<Vec<Vec2>>, Vec<Segment2>), &'static str> {
    let mut vertices = Vec::<Vec2>::new();
    let mut edge_vertices = Vec::with_capacity(segments.len());
    for segment in segments {
        if segment.start.distance(segment.end) <= epsilon {
            return Err("A selected cut edge collapses within the active tolerance.");
        }
        let start = endpoint_vertex(segment.start, &mut vertices, epsilon)?;
        let end = endpoint_vertex(segment.end, &mut vertices, epsilon)?;
        if start == end {
            return Err("A selected cut edge collapses within the active tolerance.");
        }
        edge_vertices.push([start, end]);
    }
    let mut adjacency = vec![Vec::<usize>::new(); vertices.len()];
    for (edge, [start, end]) in edge_vertices.iter().copied().enumerate() {
        adjacency[start].push(edge);
        adjacency[end].push(edge);
    }
    if adjacency.iter().any(|edges| edges.len() > 2) {
        return Err("Selected cut edges branch or touch ambiguously.");
    }

    let mut visited_edges = BTreeSet::new();
    let mut loops = Vec::new();
    let mut open_segments = Vec::new();
    for start_edge in 0..segments.len() {
        if visited_edges.contains(&start_edge) {
            continue;
        }
        let mut component_edges = Vec::new();
        let mut queue = VecDeque::from([start_edge]);
        while let Some(edge) = queue.pop_front() {
            if !visited_edges.insert(edge) {
                continue;
            }
            component_edges.push(edge);
            for vertex in edge_vertices[edge] {
                queue.extend(adjacency[vertex].iter().copied());
            }
        }
        let component_vertices = component_edges
            .iter()
            .flat_map(|edge| edge_vertices[*edge])
            .collect::<BTreeSet<_>>();
        if component_vertices
            .iter()
            .any(|vertex| adjacency[*vertex].len() != 2)
        {
            open_segments.extend(component_edges.into_iter().map(|edge| segments[edge]));
            continue;
        }
        let first = component_edges[0];
        let first_vertex = edge_vertices[first][0];
        let mut points = vec![vertices[first_vertex]];
        let mut current_vertex = first_vertex;
        let mut current_edge = first;
        loop {
            let [a, b] = edge_vertices[current_edge];
            let next_vertex = if a == current_vertex { b } else { a };
            if next_vertex == first_vertex {
                break;
            }
            points.push(vertices[next_vertex]);
            current_vertex = next_vertex;
            let Some(next_edge) = adjacency[current_vertex]
                .iter()
                .copied()
                .find(|edge| *edge != current_edge)
            else {
                return Err("Selected cut loop could not be ordered safely.");
            };
            current_edge = next_edge;
            if points.len() > component_edges.len() {
                return Err("Selected cut loop contains a repeated vertex.");
            }
        }
        if points.len() < 3 || loop_self_intersects(&points, epsilon) {
            return Err("Selected cut loop is degenerate, self-intersecting, or touching.");
        }
        loops.push(points);
    }
    for first in 0..loops.len() {
        for second in first + 1..loops.len() {
            if loops_intersect(&loops[first], &loops[second], epsilon) {
                return Err("Selected cut loops touch or overlap within the active tolerance.");
            }
        }
    }
    Ok((loops, open_segments))
}

/// Orders clipped, open selected-edge components into finite polylines.
fn order_open_paths(segments: &[Segment2], epsilon: f64) -> Result<Vec<Vec<Vec2>>, &'static str> {
    let mut vertices = Vec::<Vec2>::new();
    let mut edge_vertices = Vec::with_capacity(segments.len());
    for segment in segments {
        if segment.start.distance(segment.end) <= epsilon {
            continue;
        }
        let start = endpoint_vertex(segment.start, &mut vertices, epsilon)?;
        let end = endpoint_vertex(segment.end, &mut vertices, epsilon)?;
        if start == end {
            continue;
        }
        edge_vertices.push([start, end]);
    }

    let mut adjacency = vec![Vec::<usize>::new(); vertices.len()];
    for (edge, [start, end]) in edge_vertices.iter().copied().enumerate() {
        adjacency[start].push(edge);
        adjacency[end].push(edge);
    }
    if adjacency.iter().any(|edges| edges.len() > 2) {
        return Err("Selected finite cut paths branch or intersect ambiguously.");
    }

    let mut visited_edges = BTreeSet::new();
    let mut paths = Vec::new();
    for start_edge in 0..edge_vertices.len() {
        if visited_edges.contains(&start_edge) {
            continue;
        }
        let mut component_edges = Vec::new();
        let mut queue = VecDeque::from([start_edge]);
        while let Some(edge) = queue.pop_front() {
            if !visited_edges.insert(edge) {
                continue;
            }
            component_edges.push(edge);
            for vertex in edge_vertices[edge] {
                queue.extend(adjacency[vertex].iter().copied());
            }
        }
        let component_vertices = component_edges
            .iter()
            .flat_map(|edge| edge_vertices[*edge])
            .collect::<BTreeSet<_>>();
        let endpoints = component_vertices
            .iter()
            .copied()
            .filter(|vertex| adjacency[*vertex].len() == 1)
            .collect::<Vec<_>>();
        if endpoints.len() != 2
            || component_vertices
                .iter()
                .any(|vertex| !matches!(adjacency[*vertex].len(), 1 | 2))
        {
            return Err("Selected finite cut path could not be ordered safely.");
        }

        let mut path = vec![vertices[endpoints[0]]];
        let mut current_vertex = endpoints[0];
        let mut previous_edge = None;
        loop {
            let next_edge = adjacency[current_vertex]
                .iter()
                .copied()
                .find(|edge| Some(*edge) != previous_edge)
                .ok_or("Selected finite cut path could not be ordered safely.")?;
            let [a, b] = edge_vertices[next_edge];
            let next_vertex = if a == current_vertex { b } else { a };
            path.push(vertices[next_vertex]);
            if next_vertex == endpoints[1] {
                break;
            }
            previous_edge = Some(next_edge);
            current_vertex = next_vertex;
            if path.len() > component_edges.len() + 1 {
                return Err("Selected finite cut path contains a repeated vertex.");
            }
        }
        paths.push(path);
    }
    Ok(paths)
}

/// Closes a boundary-to-boundary polyline along one triangle-boundary arc.
fn boundary_path_region(
    path: &[Vec2],
    triangle: [Vec2; 3],
    epsilon: f64,
) -> Result<Option<PolygonSet>, &'static str> {
    if path.len() < 2 {
        return Ok(None);
    }
    let mut boundary = triangle;
    if (boundary[1] - boundary[0]).cross(boundary[2] - boundary[0]) < 0.0 {
        boundary.reverse();
    }
    let lengths = [
        boundary[1].distance(boundary[0]),
        boundary[2].distance(boundary[1]),
        boundary[0].distance(boundary[2]),
    ];
    let perimeter = lengths.iter().sum::<f64>();
    if !perimeter.is_finite() || perimeter <= epsilon {
        return Err("Selected face boundary is degenerate.");
    }
    let start = boundary_parameter(path[0], boundary, lengths, epsilon)
        .ok_or("Finite cut path endpoint is not on the selected face boundary.")?;
    let end_point = *path.last().expect("paths contain endpoints");
    let end = boundary_parameter(end_point, boundary, lengths, epsilon)
        .ok_or("Finite cut path endpoint is not on the selected face boundary.")?;
    let arc_length = (start - end).rem_euclid(perimeter);
    if arc_length <= epsilon {
        return Ok(None);
    }

    let corner_parameters = [0.0, lengths[0], lengths[0] + lengths[1]];
    let mut ring = path.to_vec();
    for (index, parameter) in corner_parameters.into_iter().enumerate() {
        let parameter = if parameter <= end + epsilon {
            parameter + perimeter
        } else {
            parameter
        };
        if parameter > end + epsilon && parameter < end + arc_length - epsilon {
            ring.push(boundary[index]);
        }
    }
    if ring.len() < 3 || loop_self_intersects(&ring, epsilon) {
        return Err("Selected finite cut path is degenerate or self-intersecting.");
    }
    let region = PolygonSet {
        shapes: vec![loop_shape(&ring)],
    };
    if polygon_area(&region)? <= epsilon * epsilon {
        return Ok(None);
    }
    Ok(Some(region))
}

fn boundary_parameter(
    point: Vec2,
    triangle: [Vec2; 3],
    edge_lengths: [f64; 3],
    epsilon: f64,
) -> Option<f64> {
    let mut best = None::<(f64, f64)>;
    let mut offset = 0.0;
    for index in 0..3 {
        let start = triangle[index];
        let edge = triangle[(index + 1) % 3] - start;
        let length = edge_lengths[index];
        let parameter = ((point - start).dot(edge) / (length * length)).clamp(0.0, 1.0);
        let projected = start + edge * parameter;
        let distance = point.distance(projected);
        if distance <= epsilon && best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, offset + parameter * length));
        }
        offset += length;
    }
    best.map(|(_, parameter)| parameter.rem_euclid(edge_lengths.iter().sum()))
}

fn endpoint_vertex(
    point: Vec2,
    vertices: &mut Vec<Vec2>,
    epsilon: f64,
) -> Result<usize, &'static str> {
    let matches = vertices
        .iter()
        .enumerate()
        .filter_map(|(index, existing)| (point.distance(*existing) <= epsilon).then_some(index))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => {
            vertices.push(point);
            Ok(vertices.len() - 1)
        }
        [index] => Ok(*index),
        _ => Err("Selected cut endpoints are ambiguous within the active tolerance."),
    }
}

fn loop_self_intersects(points: &[Vec2], epsilon: f64) -> bool {
    for first in 0..points.len() {
        let a = points[first];
        let b = points[(first + 1) % points.len()];
        for second in first + 1..points.len() {
            if second == first
                || second == (first + 1) % points.len()
                || first == (second + 1) % points.len()
            {
                continue;
            }
            if segments_intersect(
                a,
                b,
                points[second],
                points[(second + 1) % points.len()],
                epsilon,
            ) {
                return true;
            }
        }
    }
    false
}

fn loops_intersect(first: &[Vec2], second: &[Vec2], epsilon: f64) -> bool {
    (0..first.len()).any(|index| {
        let a = first[index];
        let b = first[(index + 1) % first.len()];
        (0..second.len()).any(|other| {
            segments_intersect(
                a,
                b,
                second[other],
                second[(other + 1) % second.len()],
                epsilon,
            )
        })
    })
}

fn segments_intersect(a: Vec2, b: Vec2, c: Vec2, d: Vec2, epsilon: f64) -> bool {
    let ab = b - a;
    let cd = d - c;
    let denominator = ab.cross(cd);
    let scale = ab.length().max(cd.length()).max(1.0);
    if denominator.abs() <= epsilon * scale {
        return point_segment_distance(a, c, d) <= epsilon
            || point_segment_distance(b, c, d) <= epsilon
            || point_segment_distance(c, a, b) <= epsilon
            || point_segment_distance(d, a, b) <= epsilon;
    }
    let t = (c - a).cross(cd) / denominator;
    let u = (c - a).cross(ab) / denominator;
    let t_epsilon = epsilon / ab.length().max(epsilon);
    let u_epsilon = epsilon / cd.length().max(epsilon);
    (-t_epsilon..=1.0 + t_epsilon).contains(&t) && (-u_epsilon..=1.0 + u_epsilon).contains(&u)
}

fn point_segment_distance(point: Vec2, start: Vec2, end: Vec2) -> f64 {
    let edge = end - start;
    let length_squared = edge.length_squared();
    if length_squared == 0.0 {
        return point.distance(start);
    }
    let parameter = ((point - start).dot(edge) / length_squared).clamp(0.0, 1.0);
    point.distance(start + edge * parameter)
}

fn clip_segment_to_triangle(
    segment: Segment2,
    triangle: [Vec2; 3],
    epsilon: f64,
) -> Option<(Vec2, Vec2)> {
    let orientation = ((triangle[1] - triangle[0]).cross(triangle[2] - triangle[0])).signum();
    let delta = segment.end - segment.start;
    let mut low: f64 = 0.0;
    let mut high: f64 = 1.0;
    for index in 0..3 {
        let a = triangle[index];
        let edge = triangle[(index + 1) % 3] - a;
        let start_side = edge.cross(segment.start - a) * orientation;
        let end_side = edge.cross(segment.end - a) * orientation;
        let boundary_tolerance = epsilon * edge.length();
        if start_side < -boundary_tolerance && end_side < -boundary_tolerance {
            return None;
        }
        if (start_side < -boundary_tolerance) != (end_side < -boundary_tolerance) {
            let parameter = start_side / (start_side - end_side);
            if start_side < -boundary_tolerance {
                low = low.max(parameter);
            } else {
                high = high.min(parameter);
            }
        }
    }
    (high - low > 1.0e-12).then_some((segment.start + delta * low, segment.start + delta * high))
}

fn on_triangle_boundary(point: Vec2, triangle: [Vec2; 3], epsilon: f64) -> bool {
    (0..3).any(|index| {
        point_segment_distance(point, triangle[index], triangle[(index + 1) % 3]) <= epsilon
    })
}

fn split_region_by_line(
    region: &PolygonSet,
    point: Vec2,
    direction: Vec2,
    bounds: [Vec2; 3],
    epsilon: f64,
) -> Result<(PolygonSet, PolygonSet), &'static str> {
    let Some(direction) = direction.try_normalized(1.0e-14) else {
        return Ok((region.clone(), PolygonSet::default()));
    };
    let min_x = bounds
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = bounds
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = bounds
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = bounds
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let corners = [
        Vec2::new(min_x, min_y),
        Vec2::new(max_x, min_y),
        Vec2::new(max_x, max_y),
        Vec2::new(min_x, max_y),
    ];
    let normal = Vec2::new(-direction.y, direction.x);
    let positive = clip_halfspace(&corners, point, normal, true, epsilon);
    let negative = clip_halfspace(&corners, point, normal, false, epsilon);
    if positive.len() < 3 || negative.len() < 3 {
        return Ok((region.clone(), PolygonSet::default()));
    }
    let positive = PolygonSet {
        shapes: vec![PolygonShape {
            contours: vec![positive],
        }],
    };
    let negative = PolygonSet {
        shapes: vec![PolygonShape {
            contours: vec![negative],
        }],
    };
    let inside_positive = region
        .overlay(&positive, PolygonOperation::Intersection)
        .map_err(|_| "Cut segment could not be clipped safely.")?;
    let inside_negative = region
        .overlay(&negative, PolygonOperation::Intersection)
        .map_err(|_| "Cut segment could not be clipped safely.")?;
    Ok((inside_positive, inside_negative))
}

fn clip_halfspace(
    points: &[Vec2],
    origin: Vec2,
    normal: Vec2,
    keep_positive: bool,
    epsilon: f64,
) -> Vec<Vec2> {
    let mut output = Vec::new();
    let mut previous = *points.last().expect("halfspace rectangle is nonempty");
    let mut previous_distance = (previous - origin).dot(normal);
    for current in points.iter().copied() {
        let current_distance = (current - origin).dot(normal);
        let previous_inside = if keep_positive {
            previous_distance >= -epsilon
        } else {
            previous_distance <= epsilon
        };
        let current_inside = if keep_positive {
            current_distance >= -epsilon
        } else {
            current_distance <= epsilon
        };
        if previous_inside != current_inside {
            let amount = previous_distance / (previous_distance - current_distance);
            output.push(previous + (current - previous) * amount);
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_distance = current_distance;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{CutSegment3, CutSourceTriangle, CutTool, cut_selected_faces};
    use crate::document::Rgba8;
    use crate::geometry::{GeometryTolerance, Vec3};

    fn tolerance() -> GeometryTolerance {
        GeometryTolerance::new(1.0e-7, 1.0e-12).unwrap()
    }

    fn face(source: usize, points: [Vec3; 3], color: Rgba8) -> CutSourceTriangle {
        CutSourceTriangle {
            source,
            vertices: points,
            color,
        }
    }

    fn triangle_area(vertices: [Vec3; 3]) -> f64 {
        (vertices[1] - vertices[0])
            .cross(vertices[2] - vertices[0])
            .length()
            * 0.5
    }

    fn area(result: &super::CutResult) -> f64 {
        result
            .triangles
            .iter()
            .map(|triangle| triangle_area(triangle.vertices))
            .sum()
    }

    fn loop_segments(points: &[Vec3]) -> Vec<CutSegment3> {
        (0..points.len())
            .map(|index| CutSegment3 {
                start: points[index],
                end: points[(index + 1) % points.len()],
            })
            .collect()
    }

    fn square_face() -> Vec<CutSourceTriangle> {
        let color = Rgba8 {
            red: 20,
            green: 70,
            blue: 180,
            alpha: 255,
        };
        vec![
            face(
                10,
                [Vec3::ZERO, Vec3::X * 4.0, Vec3::new(4.0, 0.0, 4.0)],
                color,
            ),
            face(
                11,
                [Vec3::ZERO, Vec3::new(4.0, 0.0, 4.0), Vec3::Z * 4.0],
                color,
            ),
        ]
    }

    #[test]
    fn holes_removes_disconnected_nested_and_concave_loop_regions_and_keeps_color() {
        let faces = square_face();
        let outer = [
            Vec3::new(0.6, 0.0, 0.6),
            Vec3::new(3.4, 0.0, 0.6),
            Vec3::new(3.4, 0.0, 3.4),
            Vec3::new(2.4, 0.0, 3.4),
            Vec3::new(2.4, 0.0, 1.8),
            Vec3::new(1.4, 0.0, 1.8),
            Vec3::new(1.4, 0.0, 3.4),
            Vec3::new(0.6, 0.0, 3.4),
        ];
        let nested = [
            Vec3::new(0.8, 0.0, 0.8),
            Vec3::new(1.1, 0.0, 0.8),
            Vec3::new(1.1, 0.0, 1.1),
            Vec3::new(0.8, 0.0, 1.1),
        ];
        let disconnected = [
            Vec3::new(3.55, 0.0, 0.3),
            Vec3::new(3.85, 0.0, 0.3),
            Vec3::new(3.85, 0.0, 0.6),
            Vec3::new(3.55, 0.0, 0.6),
        ];
        let segments = loop_segments(&outer)
            .into_iter()
            .chain(loop_segments(&nested))
            .chain(loop_segments(&disconnected))
            .collect::<Vec<_>>();
        let result = cut_selected_faces(&faces, &segments, CutTool::Holes, tolerance()).unwrap();
        assert!(result.replaced_sources.contains(&10));
        assert!(result.replaced_sources.contains(&11));
        assert!(
            (area(&result) - (16.0 - 6.24 - 0.09)).abs() < 1.0e-5,
            "actual retained area was {}",
            area(&result)
        );
        assert!(
            result
                .triangles
                .iter()
                .all(|triangle| triangle.color == faces[0].color)
        );
    }

    #[test]
    fn holes2_partitions_nested_and_concave_loops_without_losing_area() {
        let faces = square_face();
        let outer = [
            Vec3::new(0.6, 0.0, 0.6),
            Vec3::new(3.4, 0.0, 0.6),
            Vec3::new(3.4, 0.0, 3.4),
            Vec3::new(2.4, 0.0, 3.4),
            Vec3::new(2.4, 0.0, 1.8),
            Vec3::new(1.4, 0.0, 1.8),
            Vec3::new(1.4, 0.0, 3.4),
            Vec3::new(0.6, 0.0, 3.4),
        ];
        let inner = [
            Vec3::new(0.8, 0.0, 0.8),
            Vec3::new(1.1, 0.0, 0.8),
            Vec3::new(1.1, 0.0, 1.1),
            Vec3::new(0.8, 0.0, 1.1),
        ];
        let segments = loop_segments(&outer)
            .into_iter()
            .chain(loop_segments(&inner))
            .collect::<Vec<_>>();
        let result = cut_selected_faces(&faces, &segments, CutTool::Holes2, tolerance()).unwrap();
        assert!((area(&result) - 16.0).abs() < 1.0e-5);
        assert!(result.triangles.len() > 4);
        assert!(
            result
                .triangles
                .iter()
                .all(|triangle| triangle.color == faces[0].color)
        );
    }

    #[test]
    fn holes2_splits_a_finite_boundary_chord_and_ignores_a_dangling_segment() {
        let face = face(1, [Vec3::ZERO, Vec3::X * 4.0, Vec3::Z * 4.0], Rgba8::WHITE);
        let chord = CutSegment3 {
            start: Vec3::new(2.0, 0.0, 0.0),
            end: Vec3::new(0.0, 0.0, 2.0),
        };
        let dangling = CutSegment3 {
            start: Vec3::new(0.5, 0.0, 0.5),
            end: Vec3::new(0.75, 0.0, 0.75),
        };
        let result =
            cut_selected_faces(&[face], &[chord, dangling], CutTool::Holes2, tolerance()).unwrap();
        assert!((area(&result) - 8.0).abs() < 1.0e-6);
        assert!(result.triangles.len() >= 2);
    }

    #[test]
    fn holes2_splits_a_bent_finite_path_between_face_boundaries() {
        let face = face(1, [Vec3::ZERO, Vec3::X * 4.0, Vec3::Z * 4.0], Rgba8::WHITE);
        let segments = [
            CutSegment3 {
                start: Vec3::X * 3.0,
                end: Vec3::new(1.0, 0.0, 1.0),
            },
            CutSegment3 {
                start: Vec3::new(1.0, 0.0, 1.0),
                end: Vec3::Z * 3.0,
            },
        ];
        let result = cut_selected_faces(&[face], &segments, CutTool::Holes2, tolerance()).unwrap();
        assert!((area(&result) - 8.0).abs() < 1.0e-6);
        assert!(result.triangles.len() >= 3);
        assert!(
            result
                .triangles
                .iter()
                .all(|triangle| triangle.color == Rgba8::WHITE)
        );
    }

    #[test]
    fn holes_extends_an_intersecting_open_segment_to_the_selected_face_boundary() {
        let face = face(1, [Vec3::ZERO, Vec3::X * 4.0, Vec3::Z * 4.0], Rgba8::WHITE);
        let internal = CutSegment3 {
            start: Vec3::new(0.5, 0.0, 0.5),
            end: Vec3::new(1.0, 0.0, 1.0),
        };
        let result = cut_selected_faces(&[face], &[internal], CutTool::Holes, tolerance()).unwrap();
        assert!((area(&result) - 8.0).abs() < 1.0e-6);
        assert!(result.triangles.len() >= 2);
    }

    #[test]
    fn cut_tolerance_is_independent_of_large_model_translation() {
        let origin = Vec3::new(1.0e9, -2.0e9, 3.0e9);
        let face = face(
            1,
            [
                origin,
                origin + Vec3::X * 4.0,
                origin + Vec3::X * 4.0 + Vec3::Z * 4.0,
            ],
            Rgba8::WHITE,
        );
        let points = [
            origin + Vec3::new(0.5, 0.0, 0.5),
            origin + Vec3::new(1.5, 0.0, 0.5),
            origin + Vec3::new(1.5, 0.0, 1.5),
            origin + Vec3::new(0.5, 0.0, 1.5),
        ];
        let result = cut_selected_faces(
            &[face],
            &loop_segments(&points),
            CutTool::Holes2,
            tolerance(),
        )
        .unwrap();
        assert!((area(&result) - 8.0).abs() < 1.0e-5);
        assert!(result.triangles.len() > 1);
    }

    #[test]
    fn cutout_extends_selected_segments_to_infinite_planes() {
        let face = face(1, [Vec3::ZERO, Vec3::X * 4.0, Vec3::Z * 4.0], Rgba8::WHITE);
        let segment = CutSegment3 {
            start: Vec3::new(1.0, 0.0, 1.0),
            end: Vec3::new(2.0, 0.0, 2.0),
        };
        let result = cut_selected_faces(&[face], &[segment], CutTool::Cutout, tolerance()).unwrap();
        assert!((area(&result) - 8.0).abs() < 1.0e-6);
        assert!(result.triangles.len() >= 2);
    }

    #[test]
    fn touching_loops_and_invalid_tolerance_fail_before_returning_replacements() {
        let face = face(1, [Vec3::ZERO, Vec3::X * 4.0, Vec3::Z * 4.0], Rgba8::WHITE);
        let first = loop_segments(&[
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(1.0, 0.0, 0.5),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(0.5, 0.0, 1.0),
        ]);
        let second = loop_segments(&[
            Vec3::new(1.0, 0.0, 0.5),
            Vec3::new(1.5, 0.0, 0.5),
            Vec3::new(1.5, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
        ]);
        assert!(
            cut_selected_faces(
                &[face],
                &first.into_iter().chain(second).collect::<Vec<_>>(),
                CutTool::Holes2,
                tolerance()
            )
            .is_err()
        );
        assert!(
            cut_selected_faces(
                &[face],
                &loop_segments(&[Vec3::ZERO, Vec3::X, Vec3::Z]),
                CutTool::Holes,
                GeometryTolerance::new(0.0, 0.0).unwrap(),
            )
            .is_err()
        );
    }
}

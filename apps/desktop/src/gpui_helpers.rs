//! Construction helpers share the canvas projection and respect the point-input snap radius.
use super::*;

fn hit_plane(origin: Vec3, ray: Vec3, point: Vec3, normal: Vec3) -> Option<Vec3> {
    let denominator = ray.dot(normal);
    if denominator.abs() < 1.0e-10 {
        return None;
    }
    let depth = (point - origin).dot(normal) / denominator;
    (depth >= 0.0 && depth.is_finite()).then_some(origin + ray * depth)
}

fn basis(normal: Vec3) -> (Vec3, Vec3) {
    let guide = ConstructionGuide::at_point(ConstructionGuideKind::Grid, Vec3::ZERO, normal)
        .expect("normalized workplane normal");
    (guide.axis_u, guide.axis_v)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn snap(
    camera: OrbitCamera,
    size: (f32, f32),
    pointer: (f32, f32),
    origin: Vec3,
    ray: Vec3,
    spacing: f64,
    radius: f32,
    normal_axis: GridNormalAxis,
    guides: &[ConstructionGuide],
    geometry: Option<SnapResult>,
) -> Option<SnapResult> {
    if geometry.is_some_and(|snap| matches!(snap.kind, SnapKind::Endpoint | SnapKind::Line)) {
        return geometry;
    }
    if !spacing.is_finite() || spacing <= 0.0 {
        return geometry;
    }
    let normal = geometry.map_or(normal_axis.vector(), |snap| snap.normal);
    let hit = geometry
        .map(|snap| snap.point)
        .or_else(|| hit_plane(origin, ray, Vec3::ZERO, normal));
    let mut best: Option<(u8, f32, SnapResult)> = None;
    let face_depth = geometry.map(|snap| (snap.point - origin).dot(ray));
    let mut consider = |point: Vec3, normal: Vec3, kind: SnapKind, priority: u8| {
        // Helpers behind the nearest solid surface must not capture point input.
        if face_depth.is_some_and(|depth| (point - origin).dot(ray) > depth + spacing * 0.02) {
            return;
        }
        let Some(screen) = project_point(camera, size, point) else {
            return;
        };
        let distance = ((screen.0 - pointer.0).powi(2) + (screen.1 - pointer.1).powi(2)).sqrt();
        if distance <= radius
            && best
                .as_ref()
                .is_none_or(|(p, d, _)| (priority, distance) < (*p, *d))
        {
            best = Some((
                priority,
                distance,
                SnapResult {
                    point,
                    normal,
                    kind,
                    screen_distance: distance,
                },
            ));
        }
    };
    let mut plane = |anchor: Vec3, normal: Vec3, u: Vec3, v: Vec3, hit: Vec3, guide: bool| {
        let relative = hit - anchor;
        let (x, y) = (relative.dot(u), relative.dot(v));
        let (gx, gy) = (
            (x / spacing).round() * spacing,
            (y / spacing).round() * spacing,
        );
        consider(
            anchor + u * gx + v * gy,
            normal,
            if guide {
                SnapKind::GridGuide
            } else {
                SnapKind::Grid
            },
            if guide { 0 } else { 1 },
        );
        for point in [anchor + u * gx + v * y, anchor + u * x + v * gy] {
            consider(point, normal, SnapKind::GridLine, if guide { 1 } else { 2 });
        }
    };
    let (u, v) = basis(normal);
    // A surface lattice is anchored to world coordinates projected onto that surface.
    if let Some(hit) = hit {
        plane(normal * hit.dot(normal), normal, u, v, hit, false);
    }
    for guide in guides
        .iter()
        .filter(|g| g.kind == ConstructionGuideKind::Grid)
    {
        for (n, u, v) in [
            (guide.axis_w, guide.axis_u, guide.axis_v),
            (guide.axis_v, guide.axis_u, guide.axis_w),
            (guide.axis_u, guide.axis_v, guide.axis_w),
        ] {
            if let Some(hit) = hit_plane(origin, ray, guide.origin, n) {
                let local = hit - guide.origin;
                if local.dot(u).abs() <= spacing * 12.0 && local.dot(v).abs() <= spacing * 12.0 {
                    plane(guide.origin, n, u, v, hit, true);
                }
            }
        }
    }
    for guide in guides
        .iter()
        .filter(|g| g.kind == ConstructionGuideKind::Axis)
    {
        for axis in [guide.axis_u, guide.axis_v, guide.axis_w] {
            let w = origin - guide.origin;
            let dot = ray.dot(axis);
            let denominator = 1.0 - dot * dot;
            if denominator.abs() < 1.0e-10 {
                continue;
            }
            let along = (w.dot(axis) - dot * w.dot(ray)) / denominator;
            if along.abs() > 64.0 * spacing {
                continue;
            }
            let rounded = (along / spacing).round() * spacing;
            consider(
                guide.origin + axis * rounded,
                normal,
                SnapKind::AxisGuide,
                0,
            );
            consider(guide.origin + axis * along, normal, SnapKind::AxisGuide, 1);
        }
    }
    best.map(|(_, _, snap)| snap).or(geometry).or_else(|| {
        hit.map(|point| SnapResult {
            point,
            normal,
            kind: SnapKind::Free,
            screen_distance: 0.0,
        })
    })
}

pub(super) fn draw_point_field(
    scene: &mut impl RaylibDraw3D,
    point: Vec3,
    axes: [Vec3; 3],
    spacing: f64,
) {
    let [u, v, n] = axes;
    let center = u * ((point.dot(u) / spacing).round() * spacing)
        + v * ((point.dot(v) / spacing).round() * spacing)
        + n * point.dot(n);
    let radius = spacing * 0.025;
    // Tiny crosses batch as lines, avoiding hundreds of per-point sphere meshes.
    for x in -6..=6 {
        for y in -6..=6 {
            let p = center
                + u * (f64::from(x) * spacing)
                + v * (f64::from(y) * spacing)
                + n * (spacing * 0.002);
            let color = Color::new(95, 119, 143, 150);
            scene.draw_line3D(ray_vec3(p - u * radius), ray_vec3(p + u * radius), color);
            scene.draw_line3D(ray_vec3(p - v * radius), ray_vec3(p + v * radius), color);
        }
    }
}

pub(super) fn draw_guides(
    scene: &mut impl RaylibDraw3D,
    guides: &[ConstructionGuide],
    spacing: f64,
) {
    for guide in guides {
        for (axis, color) in [guide.axis_u, guide.axis_v, guide.axis_w].into_iter().zip([
            Color::new(196, 74, 67, 255),
            Color::new(57, 151, 96, 255),
            Color::new(62, 112, 194, 255),
        ]) {
            scene.draw_line3D(
                ray_vec3(guide.origin - axis * 64.0 * spacing),
                ray_vec3(guide.origin + axis * 64.0 * spacing),
                color,
            );
        }
        if guide.kind == ConstructionGuideKind::Grid {
            for (u, v) in [
                (guide.axis_u, guide.axis_v),
                (guide.axis_u, guide.axis_w),
                (guide.axis_v, guide.axis_w),
            ] {
                for i in -12..=12 {
                    for (a, b) in [(u, v), (v, u)] {
                        let p = guide.origin + a * (f64::from(i) * spacing);
                        scene.draw_line3D(
                            ray_vec3(p - b * 12.0 * spacing),
                            ray_vec3(p + b * 12.0 * spacing),
                            Color::new(116, 141, 162, 180),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snap_at(
        point: Vec3,
        guides: &[ConstructionGuide],
        geometry: Option<SnapResult>,
    ) -> SnapResult {
        let camera = OrbitCamera::default();
        let size = (1200.0, 800.0);
        let pointer = project_point(camera, size, point).unwrap();
        let (origin, ray) = camera.ray(pointer.0, pointer.1, size.0, size.1).unwrap();
        snap(
            camera,
            size,
            pointer,
            origin,
            ray,
            1.0,
            8.0,
            GridNormalAxis::Y,
            guides,
            geometry,
        )
        .unwrap()
    }
    #[test]
    fn grid_intersections_lines_and_free_input_are_distinct() {
        assert_eq!(
            snap_at(Vec3::new(1.0, 0.0, 2.0), &[], None).kind,
            SnapKind::Grid
        );
        let line = snap_at(Vec3::new(1.01, 0.0, 2.4), &[], None);
        assert_eq!(line.kind, SnapKind::GridLine);
        assert!((line.point.x - 1.0).abs() < 1.0e-6);
        assert!((line.point.z - 2.4).abs() < 0.02);
        assert_eq!(
            snap_at(Vec3::new(1.4, 0.0, 2.4), &[], None).kind,
            SnapKind::Free
        );
    }
    #[test]
    fn snapping_respects_each_workplane_and_tilted_guide_bases() {
        let camera = OrbitCamera::default();
        let size = (1200.0, 800.0);
        for (normal, point) in [
            (GridNormalAxis::X, Vec3::new(0.0, 2.0, 3.0)),
            (GridNormalAxis::Y, Vec3::new(1.0, 0.0, 2.0)),
            (GridNormalAxis::Z, Vec3::new(1.0, 2.0, 0.0)),
        ] {
            let pointer = project_point(camera, size, point).unwrap();
            let (origin, ray) = camera.ray(pointer.0, pointer.1, size.0, size.1).unwrap();
            let hit = snap(
                camera,
                size,
                pointer,
                origin,
                ray,
                1.0,
                8.0,
                normal,
                &[],
                None,
            )
            .unwrap();
            assert_eq!(hit.kind, SnapKind::Grid);
            assert!(hit.point.distance(point) < 1.0e-5);
        }
        let guide = ConstructionGuide::at_point(
            ConstructionGuideKind::Grid,
            Vec3::new(0.3, 2.0, 0.3),
            Vec3::new(0.0, 1.0, 1.0),
        )
        .unwrap();
        let point = guide.origin + guide.axis_u + guide.axis_v * 2.0;
        let hit = snap_at(point, &[guide], None);
        assert_eq!(hit.kind, SnapKind::GridGuide);
        assert!(hit.point.distance(point) < 1.0e-5);
    }

    #[test]
    fn translated_guide_lattice_and_axis_snap() {
        let guide = ConstructionGuide::at_point(
            ConstructionGuideKind::Grid,
            Vec3::new(0.3, 2.0, 0.3),
            Vec3::Y,
        )
        .unwrap();
        assert_eq!(
            snap_at(Vec3::new(1.3, 2.0, 2.3), &[guide], None).kind,
            SnapKind::GridGuide
        );
        let guide = ConstructionGuide::at_point(
            ConstructionGuideKind::Axis,
            Vec3::new(0.3, 2.0, 0.3),
            Vec3::Y,
        )
        .unwrap();
        assert_eq!(
            snap_at(guide.origin + guide.axis_u * 1.4, &[guide], None).kind,
            SnapKind::AxisGuide
        );
    }
    #[test]
    fn geometry_endpoints_take_precedence_and_surface_grid_stays_on_face() {
        let point = Vec3::new(1.0, 2.0, 1.4);
        let endpoint = SnapResult {
            point,
            normal: Vec3::Y,
            kind: SnapKind::Endpoint,
            screen_distance: 0.0,
        };
        assert_eq!(snap_at(point, &[], Some(endpoint)), endpoint);
        let surface = SnapResult {
            kind: SnapKind::Face,
            ..endpoint
        };
        let snapped = snap_at(point, &[], Some(surface));
        assert_eq!(snapped.kind, SnapKind::GridLine);
        assert_eq!(snapped.point.y, 2.0);
    }
}

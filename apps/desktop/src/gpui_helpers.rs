//! Construction helpers share the canvas projection and respect the point-input snap radius.
use super::*;

/// A world-space 2 × 2 × 2 cross, visible even when the orbit target is inside a solid.
pub(super) fn draw_camera_target(scene: &mut impl RaylibDraw3D, camera: OrbitCamera, height: f32) {
    unsafe {
        raylib::ffi::rlDrawRenderBatchActive();
        raylib::ffi::rlDisableDepthTest();
        raylib::ffi::rlDisableBackfaceCulling();
    }
    let axes = [
        (Vec3::X, Color::new(217, 69, 62, 255)),
        (Vec3::Y, Color::new(51, 155, 94, 255)),
        (Vec3::Z, Color::new(58, 112, 204, 255)),
    ];
    // The outline keeps the marker legible against both light faces and dark linework.
    // Only stroke thickness is in pixels; the endpoints stay one model unit from the target.
    for outline in [true, false] {
        for (axis, color) in axes {
            lighting::stroke(
                scene,
                [camera.target - axis, camera.target + axis],
                if outline {
                    Color::new(245, 248, 252, 255)
                } else {
                    color
                },
                if outline { 5.0 } else { 3.0 },
                camera,
                height,
            );
        }
    }
    unsafe {
        raylib::ffi::rlDrawRenderBatchActive();
        raylib::ffi::rlEnableDepthTest();
        raylib::ffi::rlEnableBackfaceCulling();
    }
}

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
    if geometry.is_some_and(|snap| matches!(snap.kind, SnapKind::Endpoint | SnapKind::Midpoint)) {
        return geometry;
    }
    if !spacing.is_finite() || spacing <= 0.0 {
        return geometry;
    }
    let surface = geometry.filter(|s| s.kind == SnapKind::Face);
    let normal = geometry.map_or(normal_axis.vector(), |snap| snap.normal);
    let hit = surface.map(|snap| snap.point).or_else(|| {
        hit_plane(
            origin,
            ray,
            geometry.map_or(Vec3::ZERO, |snap| snap.point),
            normal,
        )
    });
    let mut best: Option<(u8, f32, SnapResult)> = None;
    let face_depth = surface.map(|snap| (snap.point - origin).dot(ray));
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
    best.filter(|(_, _, s)| {
        geometry.is_none_or(|g| g.kind != SnapKind::Line)
            || matches!(
                s.kind,
                SnapKind::Grid | SnapKind::GridGuide | SnapKind::AxisGuide
            )
    })
    .map(|(_, _, snap)| snap)
    .or(geometry)
    .or_else(|| {
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
    camera: OrbitCamera,
    height: f32,
) {
    let [u, v, n] = axes;
    let center = u * ((point.dot(u) / spacing).round() * spacing)
        + v * ((point.dot(v) / spacing).round() * spacing)
        + n * point.dot(n);
    let forward = (camera.target - camera.position())
        .try_normalized(1e-12)
        .unwrap_or(-Vec3::Z);
    let depth = if camera.orthographic {
        f64::from(camera.distance)
    } else {
        (point - camera.position()).dot(forward).max(1e-6)
    };
    let radius = (depth * 2. * (f64::from(camera.field_of_view).to_radians() * 0.5).tan()
        / f64::from(height.max(1.))
        * 3.5)
        .max(spacing * 0.06);
    // Batched seven-pixel crosses remain legible as the camera zooms.
    for x in -6..=6 {
        for y in -6..=6 {
            let p = center
                + u * (f64::from(x) * spacing)
                + v * (f64::from(y) * spacing)
                + n * (spacing * 0.002);
            let color = Color::new(65, 99, 132, 205);
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
    #[test]
    fn grid_points_win_over_line_attraction_but_endpoints_still_win() {
        let line = SnapResult {
            point: Vec3::new(0.04, 0., 0.),
            normal: Vec3::Z,
            kind: SnapKind::Line,
            screen_distance: 0.,
        };
        let grid = snap_at(Vec3::new(0.04, 0., 0.), &[], Some(line));
        assert_eq!(grid.kind, SnapKind::Grid);
        assert!(grid.point.distance(Vec3::ZERO) < 1e-8);
        let elevated = SnapResult {
            point: Vec3::new(0.04, 2., 0.),
            normal: Vec3::Y,
            ..line
        };
        let upper = snap_at(elevated.point, &[], Some(elevated));
        assert_eq!(upper.kind, SnapKind::Grid);
        assert!(upper.point.distance(Vec3::Y * 2.) < 1e-8);
        let endpoint = SnapResult {
            kind: SnapKind::Endpoint,
            ..line
        };
        assert_eq!(
            snap_at(line.point, &[], Some(endpoint)).kind,
            SnapKind::Endpoint
        );
        let far = SnapResult {
            point: Vec3::new(0.45, 0., 0.45),
            ..line
        };
        assert_eq!(snap_at(far.point, &[], Some(far)).kind, SnapKind::Line);
    }
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

//! Canonical architecture controls shared by pointer input, preview and automation.
use super::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Control {
    SlabVertex { index: usize },
    WallBase { end: bool },
    WallTop { end: bool },
    HoleCorner { hole_id: String, corner: usize },
    FrameCorner { corner: usize },
}
impl Control {
    pub fn label(&self) -> &'static str {
        match self {
            Self::SlabVertex { .. } => "Slab profile",
            Self::WallBase { .. } => "Wall base endpoint",
            Self::WallTop { .. } => "Wall top height",
            Self::HoleCorner { .. } => "Opening corner",
            Self::FrameCorner { .. } => "Frame corner",
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ControlPoint {
    pub control: Control,
    pub point: Vec3,
    pub normal: Vec3,
    /// Shift changes wall base dragging from elevation/length to plan movement.
    pub alternate_normal: Option<Vec3>,
    pub axis: Option<Vec3>,
}
impl Element {
    pub fn controls(&self) -> Result<Vec<ControlPoint>, String> {
        let plane = self.plane()?;
        let mut result = vec![];
        let mut add = |control, point, normal, alternate_normal, axis| {
            result.push(ControlPoint {
                control,
                point,
                normal,
                alternate_normal,
                axis,
            })
        };
        match self {
            Self::Slab(s) => {
                for (index, p) in s.profile.points.iter().enumerate() {
                    add(Control::SlabVertex { index }, *p, plane.normal, None, None);
                }
            }
            Self::Wall(w) => {
                let top_axis = plane.v + plane.normal * w.inclination_deg.to_radians().tan();
                let top_normal = plane
                    .u
                    .cross(top_axis)
                    .try_normalized(EPS)
                    .ok_or("Invalid wall control plane.")?;
                for end in [false, true] {
                    let base = if end { w.end } else { w.start };
                    let height = if end { w.end_height() } else { w.height };
                    add(
                        Control::WallBase { end },
                        base + plane.normal * w.offset,
                        plane.normal,
                        Some(plane.v),
                        None,
                    );
                    add(
                        Control::WallTop { end },
                        base + plane.normal * w.offset + top_axis * height,
                        top_normal,
                        None,
                        Some(top_axis),
                    );
                }
            }
            Self::Frame(f) => {
                let (min, max) = frame_bounds(f, plane);
                for (corner, p) in [min, max].into_iter().enumerate() {
                    add(
                        Control::FrameCorner { corner },
                        plane.point(p),
                        plane.normal,
                        None,
                        None,
                    );
                }
            }
        }
        for hole in self.holes() {
            for (corner, p) in [Vec2::new(hole.u0, hole.v0), Vec2::new(hole.u1, hole.v1)]
                .into_iter()
                .enumerate()
            {
                let (point, normal) = if let Self::Wall(w) = self {
                    let delta = w.end - w.start;
                    let shear = w.inclination_deg.to_radians().tan();
                    let du =
                        plane.u - plane.normal * (delta.dot(plane.v) / delta.dot(plane.u) * shear);
                    let dv = plane.v + plane.normal * shear;
                    (
                        w.point(p)?,
                        du.cross(dv)
                            .try_normalized(EPS)
                            .ok_or("Invalid opening plane.")?,
                    )
                } else {
                    (plane.point(p), plane.normal)
                };
                add(
                    Control::HoleCorner {
                        hole_id: hole.id.clone(),
                        corner,
                    },
                    point,
                    normal,
                    None,
                    None,
                );
            }
        }
        Ok(result)
    }
}
fn frame_bounds(frame: &Frame, plane: Plane) -> (Vec2, Vec2) {
    frame.contour.iter().map(|p| plane.project(*p)).fold(
        (
            Vec2::new(f64::INFINITY, f64::INFINITY),
            Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        ),
        |(a, b), p| {
            (
                Vec2::new(a.x.min(p.x), a.y.min(p.y)),
                Vec2::new(b.x.max(p.x), b.y.max(p.y)),
            )
        },
    )
}
impl Document {
    pub fn move_architecture_control(
        &mut self,
        target: &SelectionTarget,
        control: &Control,
        world: Vec3,
    ) -> Result<(), String> {
        if !world.is_finite() {
            return Err("Control position must be finite.".into());
        }
        let mut owner = self
            .architecture_element(target)
            .ok_or("Architecture owner is missing.")?;
        let plane = owner.element.plane()?;
        let inverse = owner
            .transform
            .inverse(crate::geometry::GeometryTolerance::default())
            .ok_or("Architecture transform is singular.")?;
        let point = inverse.transform_point(world);
        let original = owner
            .element
            .controls()?
            .into_iter()
            .find(|p| &p.control == control)
            .ok_or("Architecture control no longer exists.")?
            .point;
        match (control, &mut owner.element) {
            (Control::SlabVertex { index }, Element::Slab(s)) => {
                *s.profile
                    .points
                    .get_mut(*index)
                    .ok_or("Slab vertex is missing.")? = plane.point(plane.project(point));
            }
            (Control::WallBase { end }, Element::Wall(w)) => {
                if *end {
                    w.end += point - original;
                } else {
                    w.start += point - original;
                }
            }
            (Control::WallTop { end }, Element::Wall(w)) => {
                let base = if *end { w.end } else { w.start };
                let height = (point - base).dot(plane.v);
                positive(height, "Wall end height")?;
                if *end {
                    w.end_height = Some(height);
                } else {
                    let other = w.end_height();
                    w.height = height;
                    w.end_height = Some(other);
                }
            }
            (Control::HoleCorner { hole_id, corner }, element) => {
                if *corner > 1 {
                    return Err("Opening corner must be 0 or 1.".into());
                }
                let hole = element
                    .holes_mut()?
                    .iter_mut()
                    .find(|h| h.id == *hole_id)
                    .ok_or("Opening no longer exists.")?;
                let opposite = if *corner == 0 {
                    Vec2::new(hole.u1, hole.v1)
                } else {
                    Vec2::new(hole.u0, hole.v0)
                };
                let p = plane.project(point);
                let resized = Hole::from_points(opposite, p);
                resized.polygon()?;
                hole.u0 = resized.u0;
                hole.u1 = resized.u1;
                hole.v0 = resized.v0;
                hole.v1 = resized.v1;
            }
            (Control::FrameCorner { corner }, Element::Frame(f)) => {
                if *corner > 1 {
                    return Err("Frame corner must be 0 or 1.".into());
                }
                let (min, max) = frame_bounds(f, plane);
                let p = plane.project(point);
                let (origin, old) = if *corner == 0 { (max, min) } else { (min, max) };
                let sx = (p.x - origin.x) / (old.x - origin.x);
                let sy = (p.y - origin.y) / (old.y - origin.y);
                if !sx.is_finite() || !sy.is_finite() || sx.abs() < EPS || sy.abs() < EPS {
                    return Err("Frame width and height must remain nonzero.".into());
                }
                for v in &mut f.contour {
                    let uv = plane.project(*v);
                    *v = plane.point(Vec2::new(
                        origin.x + (uv.x - origin.x) * sx,
                        origin.y + (uv.y - origin.y) * sy,
                    ));
                }
            }
            _ => return Err("Control does not belong to this architecture type.".into()),
        }
        let SelectedEntity::SemanticHandle { id, .. } = &target.entity else {
            return Err("Select an architecture owner.".into());
        };
        self.put_architecture(owner.definition, Some(id), owner.element)?;
        Ok(())
    }
    /// Replace the picked canonical element with its current realized edges and faces.
    pub fn explode_architecture(
        &mut self,
        target: &SelectionTarget,
    ) -> Result<(usize, Vec<SelectionTarget>), String> {
        let placed = self
            .architecture_element(target)
            .ok_or("Architecture owner is missing.")?;
        let SelectedEntity::SemanticHandle { id, .. } = &target.entity else {
            return Err("Select architecture to explode.".into());
        };
        let d = self
            .definitions
            .get(&placed.definition)
            .ok_or("Architecture definition is missing.")?;
        let owned = |ext: &BTreeMap<String, Value>| {
            ext.get(GENERATED)
                .and_then(|v| v.get("id"))
                .and_then(Value::as_str)
                == Some(id.as_str())
        };
        let mut geometry = d.geometry.clone();
        geometry.segments.retain(|_, s| owned(&s.extensions));
        geometry.triangles.retain(|_, t| owned(&t.extensions));
        let referenced = geometry
            .segments
            .values()
            .flat_map(|s| s.endpoints)
            .chain(geometry.triangles.values().flat_map(|t| t.vertices))
            .map(|r| r.id)
            .collect::<BTreeSet<_>>();
        geometry.vertices.retain(|id, _| referenced.contains(id));
        if geometry.is_empty() {
            return Err("Architecture has no realized geometry to explode.".into());
        }
        materialize(&mut geometry);
        let selected = geometry
            .segments
            .keys()
            .map(|id| SelectedEntity::Segment(*id))
            .chain(
                geometry
                    .triangles
                    .keys()
                    .map(|id| SelectedEntity::Triangle(*id)),
            )
            .map(|entity| SelectionTarget {
                occurrence: target.occurrence.clone(),
                entity,
            })
            .collect();
        let count = geometry.vertices.len();
        let mut draft = self.clone();
        draft.remove_architecture(target)?;
        if !draft.append_geometry_to_definition(placed.definition, geometry) {
            return Err("Cannot append exploded architecture geometry.".into());
        }
        *self = draft;
        Ok((count, selected))
    }
}

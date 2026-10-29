//! Analytic walking stations: straight flights and contour-contained corner arcs.
use super::*;

#[derive(Clone, Copy)]
enum Piece {
    Line(Vec2, Vec2),
    Arc {
        center: Vec2,
        radial: Vec2,
        turn: f64,
    },
}
impl Piece {
    fn length(self) -> f64 {
        match self {
            Self::Line(a, b) => length(b - a),
            Self::Arc { radial, turn, .. } => length(radial) * turn.abs(),
        }
    }
    fn at(self, t: f64) -> (Vec2, Vec2) {
        match self {
            Self::Line(a, b) => (a + (b - a) * t, (b - a) * (1. / length(b - a))),
            Self::Arc {
                center,
                radial,
                turn,
            } => {
                let angle = turn * t;
                let r = radial * angle.cos() + Vec2::new(-radial.y, radial.x) * angle.sin();
                (
                    center + r,
                    Vec2::new(-r.y, r.x) * (turn.signum() / length(r)),
                )
            }
        }
    }
    fn nearest(self, p: Vec2) -> (f64, Vec2, Vec2) {
        let t = match self {
            Self::Line(a, b) => (dot(p - a, b - a) / dot(b - a, b - a)).clamp(0., 1.),
            Self::Arc {
                center,
                radial,
                turn,
            } => {
                let r = p - center;
                let t = cross(radial, r).atan2(dot(radial, r)) / turn;
                if (0. ..=1.).contains(&t) {
                    t
                } else if length(p - self.at(0.).0) < length(p - self.at(1.).0) {
                    0.
                } else {
                    1.
                }
            }
        };
        let (q, tangent) = self.at(t);
        (t * self.length(), tangent, q)
    }
}

pub(super) struct WalkingLine {
    pieces: Vec<Piece>,
    pub length: f64,
}
impl WalkingLine {
    pub fn new(path: &[Vec2], ring: &[Vec2]) -> Result<Self, String> {
        // Extra collinear control points do not shorten a flight or limit the
        // corner radius; adding one must not change an otherwise identical stair.
        let mut clean = Vec::with_capacity(path.len());
        for &point in path {
            clean.push(point);
            while clean.len() >= 3 {
                let n = clean.len();
                let a = unit(clean[n - 2] - clean[n - 3])?;
                let b = unit(clean[n - 1] - clean[n - 2])?;
                if cross(a, b).abs() > EPS || dot(a, b) <= 0. {
                    break;
                }
                clean.remove(n - 2);
            }
        }
        let path = clean.as_slice();
        let mut pieces = vec![];
        let mut previous = path[0];
        for i in 1..path.len() - 1 {
            let incoming = path[i] - path[i - 1];
            let outgoing = path[i + 1] - path[i];
            let a = unit(incoming)?;
            let b = unit(outgoing)?;
            let turn = cross(a, b).atan2(dot(a, b));
            if turn.abs() < 20_f64.to_radians() {
                if length(path[i] - previous) > EPS {
                    pieces.push(Piece::Line(previous, path[i]));
                }
                previous = path[i];
                continue;
            }
            // Put the curvature center on the *inner* contour, not inside the
            // tread area. Then neighboring straight sections cannot intersect
            // inside the flight and leave a broken/zigzagged tread front.
            let inner = unit(Vec2::new(-a.y - b.y, a.x + b.x) * turn.signum())?;
            let clearance = ray_boundary(path[i], inner, ring)?;
            let radius = clearance * (turn.abs() * 0.5).cos();
            let trim = radius * (turn.abs() * 0.5).tan();
            let start = path[i] - a * trim;
            if trim <= EPS || trim > length(incoming).min(length(outgoing)) * 0.5 + EPS {
                return Err(
                    "Walking-path corners need more spacing for straight winder sections.".into(),
                );
            }
            if length(start - previous) > EPS {
                pieces.push(Piece::Line(previous, start));
            }
            let center = start + Vec2::new(-a.y, a.x) * (radius * turn.signum());
            let arc = Piece::Arc {
                center,
                radial: start - center,
                turn,
            };
            let samples = (turn.abs() / 3_f64.to_radians()).ceil() as usize;
            for j in 0..=samples {
                station(arc.at(j as f64 / samples as f64).0, ring)?;
            }
            pieces.push(arc);
            previous = path[i] + b * trim;
        }
        if length(path[path.len() - 1] - previous) > EPS {
            pieces.push(Piece::Line(previous, path[path.len() - 1]));
        }
        let length = pieces.iter().map(|p| p.length()).sum();
        Ok(Self { pieces, length })
    }
    pub fn sample(&self, mut distance: f64) -> Result<(Vec2, Vec2), String> {
        for (i, &piece) in self.pieces.iter().enumerate() {
            let length = piece.length();
            if distance <= length + EPS || i + 1 == self.pieces.len() {
                let (point, tangent) = piece.at((distance / length).clamp(0., 1.));
                let tangent = if (distance - length).abs() <= EPS && i + 1 < self.pieces.len() {
                    unit(tangent + self.pieces[i + 1].at(0.).1)?
                } else {
                    tangent
                };
                return Ok((point, tangent));
            }
            distance -= length;
        }
        Err("Walking path is empty.".into())
    }
    pub fn nearest(&self, point: Vec2) -> (f64, Vec2, Vec2) {
        let mut best = (f64::INFINITY, 0., Vec2::new(1., 0.), point);
        let mut offset = 0.;
        for &piece in &self.pieces {
            let (distance, tangent, projected) = piece.nearest(point);
            let error = dot(point - projected, point - projected);
            if error < best.0 - EPS {
                best = (error, offset + distance, tangent, projected);
            } else if (error - best.0).abs() <= EPS {
                best.1 = (best.1 + offset + distance) * 0.5;
                if let Ok(tangent) = unit(best.2 + tangent) {
                    best.2 = tangent;
                }
                best.3 = (best.3 + projected) * 0.5;
            }
            offset += piece.length();
        }
        (best.1, best.2, best.3)
    }
}

fn ray_boundary(point: Vec2, direction: Vec2, ring: &[Vec2]) -> Result<f64, String> {
    let mut nearest = f64::INFINITY;
    for (i, &a) in ring.iter().enumerate() {
        let edge = ring[(i + 1) % ring.len()] - a;
        let denominator = cross(direction, edge);
        if denominator.abs() <= EPS {
            continue;
        }
        let t = cross(a - point, edge) / denominator;
        let u = cross(a - point, direction) / denominator;
        if t > EPS && (-EPS..=1. + EPS).contains(&u) {
            nearest = nearest.min(t);
        }
    }
    if nearest.is_finite() {
        Ok(nearest)
    } else {
        Err("A walking-path corner needs clearance inside the stair contour.".into())
    }
}

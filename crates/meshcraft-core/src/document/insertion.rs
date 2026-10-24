//! Transaction-local spatial lookups for indexed mesh insertion.
//!
//! Existing geometry is indexed once, rather than rescanned for each incoming
//! vertex/edge. These bins never enter the document, history or file format.
use crate::geometry::Vec3;
use std::collections::HashMap;

pub(super) struct PointIndex<T> {
    tolerance: f64,
    bins: HashMap<[i64; 3], Vec<(Vec3, T, usize)>>,
    next_order: usize,
}

impl<T> PointIndex<T> {
    pub fn new(tolerance: f64) -> Self {
        Self {
            tolerance,
            bins: HashMap::new(),
            next_order: 0,
        }
    }

    fn cell(&self, p: Vec3) -> [i64; 3] {
        [p.x, p.y, p.z].map(|v| (v / self.tolerance).floor() as i64)
    }

    pub fn insert(&mut self, position: Vec3, value: T) {
        if !position.is_finite() {
            return;
        }
        let cell = self.cell(position);
        self.bins
            .entry(cell)
            .or_default()
            .push((position, value, self.next_order));
        self.next_order += 1;
    }

    pub fn within(&self, position: Vec3) -> impl Iterator<Item = (Vec3, &T, usize)> {
        let [x, y, z] = self.cell(position);
        (-1..=1).flat_map(move |dx| {
            (-1..=1).flat_map(move |dy| {
                (-1..=1).flat_map(move |dz| {
                    self.bins
                        .get(&[
                            x.saturating_add(dx),
                            y.saturating_add(dy),
                            z.saturating_add(dz),
                        ])
                        .into_iter()
                        .flatten()
                        .filter(move |(p, _, _)| p.distance(position) <= self.tolerance)
                        .map(|(p, value, order)| (*p, value, *order))
                })
            })
        })
    }

    pub fn nearest(&self, position: Vec3) -> Option<&T> {
        self.within(position)
            .min_by(|a, b| {
                a.0.distance(position)
                    .total_cmp(&b.0.distance(position))
                    .then_with(|| a.2.cmp(&b.2))
            })
            .map(|(_, value, _)| value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_matches_ordered_scan_across_bins_scales_and_ties() {
        for (origin, tolerance) in [(Vec3::ZERO, 1e-9), (Vec3::new(1e9, -2e9, 3e9), 1e-2)] {
            let mut index = PointIndex::new(tolerance);
            let mut points = vec![];
            for i in 0..600 {
                let v = i as f64;
                let p = origin
                    + Vec3::new((v * 0.73).sin(), (v * 0.31).cos(), (v * 0.91).sin())
                        * (tolerance * 100.);
                index.insert(p, i);
                points.push(p);
            }
            // Exact duplicate: tie resolution must follow the original insertion order,
            // independently of HashMap traversal order.
            index.insert(points[0], 600);
            points.push(points[0]);
            for p in &points {
                for offset in [
                    Vec3::ZERO,
                    Vec3::X * 0.95,
                    Vec3::new(-0.5, 0.5, 0.5),
                    Vec3::Z * -1.01,
                ] {
                    let query = *p + offset * tolerance;
                    let expected = points
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.distance(query) <= tolerance)
                        .min_by(|a, b| a.1.distance(query).total_cmp(&b.1.distance(query)))
                        .map(|(i, _)| i);
                    assert_eq!(expected, index.nearest(query).copied());
                }
            }
        }
    }
}

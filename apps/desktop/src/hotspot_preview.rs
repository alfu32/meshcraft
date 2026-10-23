//! Latest-target-only background feedback; dropping a drag never waits for JavaScript.
use meshcraft_core::{
    document::{Document, OccurrenceRef},
    geometry::Vec3,
};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};

type Edges = Arc<Vec<[Vec3; 2]>>;
pub(super) struct Response {
    pub serial: u64,
    pub result: Result<Edges, String>,
}
#[derive(Default)]
struct Pending {
    target: Option<(u64, Vec3)>,
    stopped: bool,
}
pub(super) struct Worker {
    pending: Arc<(Mutex<Pending>, Condvar)>,
    canceled: Arc<AtomicBool>,
    output: Arc<Mutex<Option<Response>>>,
    serial: u64,
}
impl Worker {
    pub fn new(source: Arc<Document>, path: OccurrenceRef, id: String, time: f64) -> Self {
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let canceled = Arc::new(AtomicBool::new(false));
        let output = Arc::new(Mutex::new(None));
        let result_slot = output.clone();
        let input = pending.clone();
        let cancel = canceled.clone();
        std::thread::spawn(move || {
            let base = source.hotspot_preview_scene(&path);
            let mut evaluator =
                crate::script_runtime::PreviewEvaluator::new(source, cancel.clone(), time);
            loop {
                let (lock, wake) = &*input;
                let mut state = lock.lock().unwrap();
                while state.target.is_none() && !state.stopped {
                    state = wake.wait(state).unwrap();
                }
                if state.stopped {
                    break;
                }
                let (serial, target) = state.target.take().unwrap();
                drop(state);
                let result = base.as_ref().map_err(Clone::clone).and_then(|base| {
                    let mut document = base.clone();
                    document.set_hotspot_target(&path, &id, target)?;
                    let document = evaluator.evaluate(document)?;
                    wireframe(&document, &path).map(Arc::new)
                });
                if cancel.load(Ordering::Acquire) {
                    break;
                }
                *result_slot.lock().unwrap() = Some(Response { serial, result });
            }
        });
        Self {
            pending,
            canceled,
            output,
            serial: 0,
        }
    }
    pub fn submit(&mut self, target: Vec3) {
        self.serial = self.serial.wrapping_add(1);
        let (lock, wake) = &*self.pending;
        lock.lock().unwrap().target = Some((self.serial, target));
        wake.notify_one();
    }
    pub fn poll(&self) -> Option<Response> {
        self.output.lock().unwrap().take()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.canceled.store(true, Ordering::Release);
        let (lock, wake) = &*self.pending;
        lock.lock().unwrap().stopped = true;
        wake.notify_one();
    }
}
fn wireframe(document: &Document, owner: &OccurrenceRef) -> Result<Vec<[Vec3; 2]>, String> {
    let mut edges = std::collections::BTreeMap::new();
    let key = |p: Vec3| [p.x, p.y, p.z].map(|v| if v == 0. { 0 } else { v.to_bits() });
    for path in document
        .scene_geometry_refs()?
        .into_iter()
        .filter(|p| p.path.starts_with(&owner.path))
    {
        let r = document
            .resolve_occurrence(&path)
            .ok_or("preview instance missing")?;
        let g = document
            .effective_geometry(&path)
            .ok_or("preview geometry missing")?;
        let world = |v| {
            g.vertex_position(v)
                .map(|p| r.world_transform.transform_point(p))
        };
        let mut insert = |a: Vec3, b: Vec3| {
            if a == b {
                return;
            }
            let (ka, kb) = (key(a), key(b));
            edges
                .entry(if ka <= kb { (ka, kb) } else { (kb, ka) })
                .or_insert([a, b]);
        };
        for segment in g.segments.values() {
            if let [Some(a), Some(b)] = segment.endpoints.map(world) {
                insert(a, b);
            }
        }
        for triangle in g.triangles.values() {
            if let [Some(a), Some(b), Some(c)] = triangle.vertices.map(world) {
                insert(a, b);
                insert(b, c);
                insert(c, a);
            }
        }
    }
    Ok(edges.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use meshcraft_core::document::hotspots::{Hotspot, TriangleAttachment};
    #[test]
    fn only_the_affected_object_is_previewed_and_latest_target_wins() {
        let mut document = Document::new();
        let path = document
            .add_cuboid("part", Vec3::new(2., 2., 2.), Vec3::ZERO)
            .unwrap();
        let r = document.resolve_occurrence(&path).unwrap();
        let id = r.definition.id;
        let attached_triangles = r
            .definition
            .geometry
            .triangles
            .values()
            .map(|t| TriangleAttachment::from_triangle(&r.definition.geometry, t))
            .collect();
        document
            .put_hotspot(
                id,
                Hotspot {
                    id: "move".into(),
                    attached_triangles,
                    ..Default::default()
                },
            )
            .unwrap();
        for i in 0..1000 {
            document
                .copy_root_occurrence(&path, Vec3::X * (10. + f64::from(i) * 3.))
                .unwrap();
        }
        let source = Arc::new(document);
        let before = serde_json::to_value(&*source).unwrap();
        let mut worker = Worker::new(source.clone(), path, "move".into(), 0.);
        for i in 1..=1000 {
            worker.submit(Vec3::X * f64::from(i));
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let edges = loop {
            if let Some(response) = worker.poll()
                && response.serial == 1000
            {
                break response.result.unwrap();
            }
            assert!(
                std::time::Instant::now() < deadline,
                "latest preview never arrived"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        let min = edges
            .iter()
            .flatten()
            .map(|p| p.x)
            .fold(f64::INFINITY, f64::min);
        let max = edges
            .iter()
            .flatten()
            .map(|p| p.x)
            .fold(f64::NEG_INFINITY, f64::max);
        assert_eq!((min, max), (1000., 1002.));
        assert_eq!(edges.len(), 18);
        assert_eq!(serde_json::to_value(&*source).unwrap(), before);
        drop(worker);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while Arc::strong_count(&source) > 1 {
            assert!(
                std::time::Instant::now() < deadline,
                "preview worker did not stop"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

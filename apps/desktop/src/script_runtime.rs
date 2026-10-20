//! Bounded latest-request worker; script failures never run on the UI/render threads.
use meshcraft_core::document::{
    Document, OccurrenceRef,
    scripts::{Drawable, STATE_KEY, ScriptFrame, ScriptState},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, BufWriter, Read, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
pub struct Request {
    pub document: Arc<Document>,
    pub query_model: Option<Arc<Document>>,
    pub key: u64,
    pub time: f64,
    pub delta: f64,
}
pub struct Response {
    pub key: u64,
    pub frames: BTreeMap<OccurrenceRef, ScriptFrame>,
    pub errors: Vec<String>,
    /// More instances remain in this sweep; partial results are valid.
    pub pending: bool,
    pub halted: bool,
}
pub struct Runtime {
    sender: Option<mpsc::SyncSender<Request>>,
    receiver: mpsc::Receiver<Response>,
    join: Option<thread::JoinHandle<()>>,
    busy: bool,
    query_model: Option<Arc<Document>>,
}
struct Worker {
    child: Child,
    broken: bool,
    input: BufWriter<std::process::ChildStdin>,
    output: mpsc::Receiver<Result<String, String>>,
    reader: Option<thread::JoinHandle<()>>,
}
impl Worker {
    fn new() -> Result<Self, String> {
        // Cargo can replace the executable while this UI session remains open. On Linux,
        // execute the running image so even an unlinked inode can launch a matching worker.
        #[cfg(target_os = "linux")]
        let executable = std::path::PathBuf::from("/proc/self/exe");
        #[cfg(not(target_os = "linux"))]
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut child = Command::new(executable)
            .arg("--script-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let input = child.stdin.take().ok_or("worker stdin missing")?;
        let stdout = child.stdout.take().ok_or("worker stdout missing")?;
        let (send, output) = mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                let result = reader
                    .by_ref()
                    .take(32_000_001)
                    .read_line(&mut line)
                    .map_err(|e| e.to_string())
                    .and_then(|n| {
                        if n == 0 {
                            Err("script worker exited".into())
                        } else if n > 32_000_000 {
                            Err("script response exceeds 32 MB".into())
                        } else {
                            Ok(line)
                        }
                    });
                let failed = result.is_err();
                // Never block shutdown if a late response is queued when the child
                // exits after its watchdog. There is only one outstanding request.
                if send.try_send(result).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            broken: false,
            input: BufWriter::with_capacity(64 * 1024, input),
            output,
            reader: Some(reader),
        })
    }
    fn evaluate(&mut self, request: Value) -> Result<Value, String> {
        let result = self.exchange(request);
        if result.is_err() {
            self.broken = true;
        }
        let result = result?;
        if let Some(error) = result.get("error") {
            return Err(error.as_str().unwrap_or("script failed").into());
        }
        result
            .get("ok")
            .cloned()
            .ok_or("worker returned no result".into())
    }
    fn exchange(&mut self, request: Value) -> Result<Value, String> {
        writeln!(self.input, "{request}")
            .and_then(|_| self.input.flush())
            .map_err(|e| e.to_string())?;
        let started = Instant::now();
        let line = loop {
            match self.output.recv_timeout(Duration::from_millis(25)) {
                Ok(result) => break result?,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("JavaScriptCore worker exited".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if started.elapsed() > Duration::from_secs(2) {
                        return Err("Script worker exceeded its two-second watchdog".into());
                    }
                    #[cfg(target_os = "linux")]
                    if let Ok(status) =
                        std::fs::read_to_string(format!("/proc/{}/status", self.child.id()))
                    {
                        let rss = status
                            .lines()
                            .find_map(|line| line.strip_prefix("VmRSS:"))
                            .and_then(|v| v.split_whitespace().next())
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or(0);
                        if rss > 512 * 1024 {
                            return Err(
                                "JavaScriptCore worker exceeded its 512 MiB memory budget".into()
                            );
                        }
                    }
                }
            }
        };
        serde_json::from_str(&line).map_err(|e| e.to_string())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
#[derive(Deserialize)]
struct Output {
    items: Vec<Drawable>,
    state: ScriptState,
}
/// All instances in a sweep see the same scene and time. A continuation advances
/// through that snapshot instead of restarting at the first occurrence every tick.
struct Sweep {
    request: Request,
    paths: Vec<OccurrenceRef>,
    next: usize,
    prepared: bool,
}
impl Sweep {
    fn new(request: Request) -> Self {
        let paths = request.document.scripted_occurrences();
        Self {
            request,
            paths,
            next: 0,
            prepared: false,
        }
    }
    fn pending(&self) -> bool {
        self.next < self.paths.len()
    }
    fn model(&self) -> Value {
        let document = &self.request.document;
        let query = self.request.query_model.as_ref().unwrap_or(document);
        let mut objects = query.scene_geometry_refs().unwrap_or_default().into_iter().filter_map(|path| {
            let resolved=query.resolve_occurrence(&path)?;
            let geometry=query.effective_geometry(&path)?;
            Some(json!({"id":format!("{:?}",path.path),"name":resolved.definition.name,"definitionId":resolved.definition.id.0,"transform":resolved.world_transform,"geometry":meshcraft_core::document::scripts::geometry_drawables(geometry)}))
        }).collect::<Vec<_>>();
        // A draft preview uses the full source model for queries and adds its own
        // isolated occurrence once, rather than cloning the full model per instance.
        if self.request.query_model.is_some() {
            for path in &self.paths {
                let id = format!("{:?}", path.path);
                if objects.iter().any(|o| o["id"] == id) {
                    continue;
                }
                if let Some(r) = document.resolve_occurrence(path)
                    && let Ok(e) = document.evaluate_hotspots(path)
                {
                    objects.push(json!({"id":id,"name":r.definition.name,"definitionId":r.definition.id.0,"transform":r.world_transform,"geometry":meshcraft_core::document::scripts::geometry_drawables(&e.geometry)}));
                }
            }
        }
        json!({"objects":objects,"materials":query.materials(),"time":self.request.time,"deltaTime":self.request.delta,"settings":query.model_settings()})
    }
    fn evaluate(
        &mut self,
        worker: &mut impl FnMut(Value) -> Result<Value, String>,
        failed: &mut BTreeMap<OccurrenceRef, String>,
        budget: Duration,
    ) -> Response {
        let mut response = Response {
            key: self.request.key,
            frames: BTreeMap::new(),
            errors: Vec::new(),
            pending: false,
            halted: false,
        };
        let started = Instant::now();
        if !self.prepared {
            if let Err(error) = worker(json!({"prepareModel":self.model()})) {
                response.errors.push(error);
                response.halted = true;
                self.next = self.paths.len();
                return response;
            }
            self.prepared = true;
        }
        let document = &self.request.document;
        let mut evaluated = false;
        while self.next < self.paths.len() {
            // Always evaluate at least one eligible instance, even after preparing
            // a large snapshot. Exhaustion is normal scheduling, never a JS error.
            if evaluated && started.elapsed() >= budget {
                break;
            }
            let path = self.paths[self.next].clone();
            self.next += 1;
            if failed.contains_key(&path) {
                continue;
            }
            let Some(r) = document.resolve_occurrence(&path) else {
                continue;
            };
            let Some(source) = document.script(r.definition.id) else {
                continue;
            };
            let state = document
                .script_frame(&path)
                .map(|f| f.state.clone())
                .or_else(|| {
                    r.edits
                        .and_then(|e| e.values.get(STATE_KEY))
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                })
                .unwrap_or_default();
            let authored = match document.evaluate_hotspots(&path) {
                Ok(e) => e.geometry,
                Err(e) => {
                    failed.insert(path, e);
                    continue;
                }
            };
            let input = json!({"id":format!("{:?}",path.path),"source":source,"state":state,
                "geometry":meshcraft_core::document::scripts::geometry_drawables(&authored),
                "hotspots":r.edits.map(|e|&e.hotspot_positions),"time":self.request.time,"deltaTime":self.request.delta});
            evaluated = true;
            match worker(input)
                .and_then(|v| {
                    serde_json::from_value::<Output>(v)
                        .map_err(|e| format!("Invalid geometry: {e}"))
                })
                .and_then(|out| ScriptFrame::build(&authored, out.items, out.state))
            {
                Ok(frame) => {
                    response.frames.insert(path, frame);
                }
                Err(e) => {
                    failed.insert(path, format!("{}: {e}", r.definition.name));
                }
            }
        }
        response.pending = self.pending();
        response.halted = !response.pending && failed.len() == self.paths.len();
        response.errors = failed.values().cloned().collect();
        response
    }
}
impl Runtime {
    pub fn with_query_model(model: Arc<Document>) -> Self {
        let mut runtime = Self::new();
        runtime.query_model = Some(model);
        runtime
    }
    pub fn new() -> Self {
        let (sender, input) = mpsc::sync_channel::<Request>(1);
        let (output, receiver) = mpsc::sync_channel(1);
        let join = thread::spawn(move || {
            let mut worker = None;
            let mut failed = BTreeMap::new();
            let mut sweep: Option<Sweep> = None;
            let mut last_time: Option<f64> = None;
            while let Ok(mut request) = input.recv() {
                if sweep.as_ref().is_none_or(|s| s.request.key != request.key) {
                    failed.clear();
                }
                if sweep
                    .as_ref()
                    .is_none_or(|s| s.request.key != request.key || !s.pending())
                {
                    if let Some(previous) = last_time {
                        request.delta = (request.time - previous).max(0.);
                    }
                    last_time = Some(request.time);
                    sweep = Some(Sweep::new(request));
                }
                if worker.is_none() {
                    worker = Worker::new().ok();
                }
                let active = sweep.as_mut().unwrap();
                let response = if let Some(w) = worker.as_mut() {
                    active.evaluate(
                        &mut |v| w.evaluate(v),
                        &mut failed,
                        Duration::from_millis(50),
                    )
                } else {
                    active.next = active.paths.len();
                    Response {
                        key: active.request.key,
                        frames: BTreeMap::new(),
                        errors: vec!["Could not start the JavaScriptCore worker".into()],
                        pending: false,
                        halted: true,
                    }
                };
                if worker.as_ref().is_some_and(|w| w.broken) {
                    worker = None;
                    active.prepared = false;
                }
                if output.send(response).is_err() {
                    break;
                }
            }
        });
        Self {
            sender: Some(sender),
            receiver,
            join: Some(join),
            busy: false,
            query_model: None,
        }
    }
    pub fn submit(&mut self, document: Arc<Document>, key: u64, time: f64, delta: f64) {
        if self.busy {
            return;
        }
        if self.sender.as_ref().is_some_and(|s| {
            s.try_send(Request {
                document,
                query_model: self.query_model.clone(),
                key,
                time,
                delta,
            })
            .is_ok()
        }) {
            self.busy = true;
        }
    }
    pub fn poll(&mut self) -> Option<Response> {
        let response = self.receiver.try_recv().ok()?;
        self.busy = false;
        Some(response)
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.sender.take();
        while self.busy {
            match self.receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => self.busy = false,
                Err(_) => {}
            }
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
pub struct Clock {
    pub start: Instant,
    pub last: Instant,
}
impl Default for Clock {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            start: now,
            last: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meshcraft_core::{document::scripts::ScriptDefinition, geometry::Vec3};

    fn sweep(count: usize) -> Sweep {
        let mut document = Document::default();
        let path = document
            .add_scripted_object("frame", ScriptDefinition::default())
            .unwrap();
        for i in 1..count {
            document
                .copy_root_occurrence(&path, Vec3::new(i as f64 * 4., 0., 0.))
                .unwrap();
        }
        Sweep::new(Request {
            document: Arc::new(document),
            query_model: None,
            key: 7,
            time: 2.,
            delta: 0.033,
        })
    }
    #[test]
    fn budget_yields_without_starving_late_instances_or_repeating_model_payload() {
        let mut sweep = sweep(64);
        let mut prepared = 0;
        let mut ids = Vec::new();
        let mut seen = BTreeMap::new();
        let mut failed = BTreeMap::new();
        let mut evaluate = |request: Value| {
            if request.get("prepareModel").is_some() {
                prepared += 1;
                assert_eq!(
                    65,
                    request["prepareModel"]["objects"].as_array().unwrap().len()
                );
                return Ok(Value::Null);
            }
            assert!(request.get("model").is_none());
            assert_eq!(2., request["time"]);
            ids.push(request["id"].clone());
            Ok(json!({"items":[],"state":{"initializer":"test","properties":{}}}))
        };
        // A zero budget forces one instance per continuation, without flaky timing.
        for i in 0..64 {
            let response = sweep.evaluate(&mut evaluate, &mut failed, Duration::ZERO);
            assert_eq!(i != 63, response.pending);
            assert!(!response.halted);
            assert!(response.errors.is_empty());
            assert_eq!(1, response.frames.len());
            seen.extend(response.frames);
        }
        assert_eq!(1, prepared);
        assert_eq!(64, ids.len());
        assert_eq!(64, seen.len());
    }
    #[test]
    fn all_instances_finish_and_retain_independent_state_across_sweeps() {
        use crate::script_engine::Session;
        let mut first = sweep(56);
        let mut document = (*first.request.document).clone();
        let definition = document
            .resolve_occurrence(&first.paths[0])
            .unwrap()
            .definition
            .id;
        document.set_script(definition,ScriptDefinition {source:r#"
function onCreate(instance,model){this.created=(this.created||0)+1;this.count=model.objects.length;}
function onAnimationFrame(){this.frames=(this.frames||0)+1;return [cube(new Point(),1,this.id),hotspot(new Point(2,0,0),'width')];}
"#.into(),..Default::default()}).unwrap();
        first = Sweep::new(Request {
            document: Arc::new(document),
            ..first.request
        });
        let mut session: Option<Session> = None;
        let mut worker = |request: Value| {
            if let Some(model) = request.get("prepareModel") {
                session = Some(Session::new(model)?);
                Ok(Value::Null)
            } else {
                session.as_ref().unwrap().evaluate(&request)
            }
        };
        let mut failed = BTreeMap::new();
        for frame_number in 1..=2 {
            let mut document = (*first.request.document).clone();
            loop {
                let response = first.evaluate(&mut worker, &mut failed, Duration::from_millis(10));
                assert!(response.errors.is_empty(), "{:?}", response.errors);
                document.install_script_frames(response.frames);
                if !response.pending {
                    break;
                }
            }
            for path in &first.paths {
                let frame = document.script_frame(path).unwrap();
                assert_eq!(12, frame.geometry.triangles.len());
                assert_eq!(1, frame.hotspots.len());
                assert_eq!(json!(1), frame.state.properties["created"]);
                assert_eq!(json!(57), frame.state.properties["count"]);
                assert_eq!(json!(frame_number), frame.state.properties["frames"]);
            }
            first = Sweep::new(Request {
                document: Arc::new(document),
                ..first.request
            });
        }
    }
    #[test]
    fn failed_instances_do_not_stop_healthy_animation_on_later_sweeps() {
        let mut sweep = sweep(3);
        let mut failed = BTreeMap::new();
        let bad = format!("{:?}", sweep.paths.last().unwrap().path);
        let mut worker = |v: Value| {
            if v.get("prepareModel").is_some() {
                return Ok(Value::Null);
            }
            if v["id"] == bad {
                return Err("interrupted".into());
            }
            Ok(json!({"items":[],"state":{"initializer":"test","properties":{}}}))
        };
        let response = sweep.evaluate(&mut worker, &mut failed, Duration::from_secs(1));
        assert_eq!(2, response.frames.len());
        assert_eq!(1, response.errors.len());
        assert!(!response.halted);
        // Retry healthy scripts, skip the already interrupted one.
        let mut next = Sweep::new(sweep.request);
        let response = next.evaluate(&mut worker, &mut failed, Duration::from_secs(1));
        assert_eq!(2, response.frames.len());
        assert!(!response.halted);
        // A failure in the last continuation must not halt the healthy scripts.
        let mut next = Sweep::new(next.request);
        next.next = 2;
        let response = next.evaluate(&mut worker, &mut failed, Duration::ZERO);
        assert!(response.frames.is_empty());
        assert!(!response.pending);
        assert!(!response.halted);
    }
}

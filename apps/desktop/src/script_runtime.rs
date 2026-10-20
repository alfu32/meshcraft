//! Bounded latest-request worker; script failures never run on the UI/render threads.
use meshcraft_core::document::{
    Document, OccurrenceRef,
    scripts::{Drawable, STATE_KEY, ScriptFrame, ScriptState},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
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
    input: std::process::ChildStdin,
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
                if send.send(result).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            output,
            reader: Some(reader),
        })
    }
    fn evaluate(&mut self, request: Value) -> Result<Value, String> {
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
        let result: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if let Some(error) = result.get("error") {
            return Err(error.as_str().unwrap_or("script failed").into());
        }
        result
            .get("ok")
            .cloned()
            .ok_or("worker returned no result".into())
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
fn evaluate(
    worker: &mut Worker,
    request: &Request,
    failed: &mut BTreeMap<OccurrenceRef, String>,
) -> Response {
    let document = &request.document;
    let query_model = request.query_model.as_ref().unwrap_or(document);
    let mut frames = BTreeMap::new();
    let mut errors = Vec::new();
    let paths = document.scripted_occurrences();
    let objects = query_model.scene_geometry_refs().unwrap_or_default().into_iter().filter_map(|path| {
        let resolved=query_model.resolve_occurrence(&path)?;
        let geometry=query_model.effective_geometry(&path)?;
        Some(json!({"id":format!("{:?}",path.path),"name":resolved.definition.name,"definitionId":resolved.definition.id.0,"transform":resolved.world_transform,"geometry":meshcraft_core::document::scripts::geometry_drawables(geometry)}))
    }).collect::<Vec<_>>();
    let model = json!({"objects":objects,"materials":query_model.materials(),"time":request.time,"deltaTime":request.delta,"settings":query_model.model_settings()});
    let started = Instant::now();
    for path in paths {
        if let Some(error) = failed.get(&path) {
            errors.push(error.clone());
            continue;
        }
        if started.elapsed() > Duration::from_secs(2) {
            errors.push("Script frame batch exceeded two seconds".into());
            break;
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
                errors.push(e);
                continue;
            }
        };
        let geometry = meshcraft_core::document::scripts::geometry_drawables(&authored);
        let instance_id = format!("{:?}", path.path);
        let mut local_model = model.clone();
        if let Some(objects) = local_model["objects"].as_array_mut()
            && !objects.iter().any(|o| o["id"] == instance_id)
        {
            objects.push(json!({"id":instance_id,"name":r.definition.name,"definitionId":r.definition.id.0,"transform":r.world_transform,"geometry":geometry}));
        }
        let input = json!({"id":instance_id,"source":source,"state":state,"geometry":geometry,"hotspots":r.edits.map(|e|&e.hotspot_positions),"model":local_model,"time":request.time,"deltaTime":request.delta});
        let result = worker
            .evaluate(input)
            .and_then(|v| {
                serde_json::from_value::<Output>(v).map_err(|e| format!("Invalid geometry: {e}"))
            })
            .and_then(|out| ScriptFrame::build(&authored, out.items, out.state));
        match result {
            Ok(frame) => {
                frames.insert(path, frame);
            }
            Err(e) => {
                let message = format!("{}: {e}", r.definition.name);
                failed.insert(path, message.clone());
                errors.push(message);
            }
        }
    }
    Response {
        key: request.key,
        frames,
        errors,
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
            let mut previous_key = None;
            while let Ok(request) = input.recv() {
                if previous_key != Some(request.key) {
                    failed.clear();
                    previous_key = Some(request.key);
                }
                if worker.is_none() {
                    worker = Worker::new().ok();
                }
                let response = if let Some(w) = worker.as_mut() {
                    evaluate(w, &request, &mut failed)
                } else {
                    Response {
                        key: request.key,
                        frames: BTreeMap::new(),
                        errors: vec!["Could not start the JavaScriptCore worker".into()],
                    }
                };
                let failed = !response.errors.is_empty();
                if output.send(response).is_err() {
                    break;
                }
                if failed {
                    worker = None;
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

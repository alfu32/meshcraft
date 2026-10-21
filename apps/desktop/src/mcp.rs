//! Bounded loopback HTTP transport. Model work is queued onto the application owner.
#[path = "mcp_protocol.rs"]
mod protocol;
pub(crate) use protocol::contract;
pub(crate) fn guide() -> &'static str {
    protocol::GUIDE
}
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
const BODY_LIMIT: usize = 8 * 1024 * 1024;
const HEADER_LIMIT: usize = 16384;
const CONNECTION_LIMIT: usize = 8;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) struct Work {
    pub method: String,
    pub params: Value,
    reply: mpsc::SyncSender<Result<Value, String>>,
    alive: Arc<AtomicBool>,
    canceled: Arc<AtomicBool>,
    deadline: Instant,
}
impl Work {
    pub fn live(&self) -> bool {
        self.alive.load(Ordering::Acquire)
            && !self.canceled.load(Ordering::Acquire)
            && Instant::now() < self.deadline
    }
    pub fn finish(self, result: Result<Value, String>) {
        let _ = self.reply.try_send(result);
    }
}

type Inflight = Arc<Mutex<BTreeMap<String, Vec<Arc<AtomicBool>>>>>;
pub(crate) struct Server {
    pub port: u16,
    receiver: mpsc::Receiver<Work>,
    alive: Arc<AtomicBool>,
    sockets: Arc<Mutex<BTreeMap<usize, TcpStream>>>,
    listener: Option<JoinHandle<()>>,
}
impl Server {
    pub fn start(port: u16) -> Result<Self, String> {
        let socket = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )
        .map_err(|e| e.to_string())?;
        socket.set_reuse_address(true).map_err(|e| e.to_string())?;
        socket
            .bind(&std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port)).into())
            .map_err(|e| e.to_string())?;
        socket.listen(16).map_err(|e| e.to_string())?;
        let listener: TcpListener = socket.into();
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let (sender, receiver) = mpsc::sync_channel(16);
        let alive = Arc::new(AtomicBool::new(true));
        let sockets = Arc::new(Mutex::new(BTreeMap::new()));
        let running = Arc::clone(&alive);
        let connections = Arc::clone(&sockets);
        let join = thread::Builder::new()
            .name("meshcraft-mcp".into())
            .spawn(move || {
                let count = Arc::new(AtomicUsize::new(0));
                let inflight = Inflight::default();
                let mut serial = 0usize;
                while running.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, peer)) => {
                            if !peer.ip().is_loopback()
                                || count.load(Ordering::Acquire) >= CONNECTION_LIMIT
                            {
                                let _ = respond(
                                    &mut stream,
                                    503,
                                    json!({"error":"MCP connection limit"}),
                                );
                                continue;
                            }
                            serial = serial.wrapping_add(1);
                            let id = serial;
                            if let Ok(socket) = stream.try_clone() {
                                connections.lock().unwrap().insert(id, socket);
                            }
                            let active = Arc::clone(&running);
                            let sockets = Arc::clone(&connections);
                            let count = Arc::clone(&count);
                            let sender = sender.clone();
                            let inflight = Arc::clone(&inflight);
                            count.fetch_add(1, Ordering::AcqRel);
                            let cleanup_count = Arc::clone(&count);
                            let cleanup_sockets = Arc::clone(&sockets);
                            if thread::Builder::new()
                                .name("meshcraft-mcp-request".into())
                                .spawn(move || {
                                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                                    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                                    let result = read_request(&mut stream).and_then(|request| {
                                        handle(request, port, &sender, active, inflight, &stream)
                                    });
                                    let (status, value) =
                                        result.unwrap_or_else(|(status, message)| {
                                            (status, json!({"error":message}))
                                        });
                                    let _ = respond(&mut stream, status, value);
                                    sockets.lock().unwrap().remove(&id);
                                    count.fetch_sub(1, Ordering::AcqRel);
                                })
                                .is_err()
                            {
                                cleanup_sockets.lock().unwrap().remove(&id);
                                cleanup_count.fetch_sub(1, Ordering::AcqRel);
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => {
                            running.store(false, Ordering::Release);
                            break;
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            port,
            receiver,
            alive,
            sockets,
            listener: Some(join),
        })
    }
    pub fn running(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }
    pub fn try_recv(&self) -> Option<Work> {
        self.receiver.try_recv().ok()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        for stream in self.sockets.lock().unwrap().values() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        if let Some(join) = self.listener.take() {
            let _ = join.join();
        }
    }
}
struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}
type HttpError = (u16, String);
fn bad(message: impl ToString) -> HttpError {
    (400, message.to_string())
}
struct TimedReader<'a> {
    stream: &'a mut TcpStream,
    deadline: Instant,
}
impl Read for TimedReader<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let left = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "HTTP request deadline")
            })?;
        self.stream
            .set_read_timeout(Some(left.min(Duration::from_secs(1))))?;
        self.stream.read(bytes)
    }
}
fn read_request(stream: &mut TcpStream) -> Result<Request, HttpError> {
    let mut reader = BufReader::new(TimedReader {
        stream,
        deadline: Instant::now() + Duration::from_secs(5),
    });
    let start = Instant::now();
    let mut bytes = Vec::new();
    loop {
        let mut line = Vec::new();
        let n = reader
            .by_ref()
            .take((HEADER_LIMIT + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(bad)?;
        if n == 0 {
            return Err(bad("incomplete HTTP headers"));
        }
        bytes.extend(&line);
        if bytes.len() > HEADER_LIMIT {
            return Err((431, "HTTP headers exceed 16 KiB".into()));
        }
        if line == b"\r\n" {
            break;
        }
        if start.elapsed() > Duration::from_secs(5) {
            return Err((408, "HTTP header timeout".into()));
        }
    }
    let mut slots = [httparse::EMPTY_HEADER; 40];
    let mut parsed = httparse::Request::new(&mut slots);
    if !parsed.parse(&bytes).map_err(bad)?.is_complete() {
        return Err(bad("incomplete headers"));
    }
    let method = parsed
        .method
        .ok_or_else(|| bad("missing method"))?
        .to_owned();
    let path = parsed.path.ok_or_else(|| bad("missing path"))?.to_owned();
    let mut headers = BTreeMap::new();
    for h in parsed.headers {
        let name = h.name.to_ascii_lowercase();
        let value = std::str::from_utf8(h.value).map_err(bad)?.trim().to_owned();
        if headers.insert(name, value).is_some() {
            return Err(bad("duplicate HTTP header"));
        }
    }
    let mut body = Vec::new();
    if let Some(encoding) = headers.get("transfer-encoding") {
        if !encoding.eq_ignore_ascii_case("chunked") || headers.contains_key("content-length") {
            return Err(bad("invalid HTTP framing"));
        }
        loop {
            if start.elapsed() > Duration::from_secs(5) {
                return Err((408, "HTTP body timeout".into()));
            }
            let mut line = String::new();
            reader
                .by_ref()
                .take(256)
                .read_line(&mut line)
                .map_err(bad)?;
            let size = usize::from_str_radix(line.split(';').next().unwrap_or("").trim(), 16)
                .map_err(bad)?;
            if size == 0 {
                // Trailers are not needed for MCP; accept the terminating blank line only.
                let mut end = [0; 2];
                reader.read_exact(&mut end).map_err(bad)?;
                if end != *b"\r\n" {
                    return Err(bad("HTTP trailers are unsupported"));
                }
                break;
            }
            if size > BODY_LIMIT - body.len() {
                return Err((413, "MCP body exceeds 8 MiB".into()));
            }
            let old = body.len();
            body.resize(old + size, 0);
            reader.read_exact(&mut body[old..]).map_err(bad)?;
            let mut end = [0; 2];
            reader.read_exact(&mut end).map_err(bad)?;
            if end != *b"\r\n" {
                return Err(bad("invalid HTTP chunk"));
            }
        }
    } else if let Some(length) = headers.get("content-length") {
        let n = length.parse::<usize>().map_err(bad)?;
        if n > BODY_LIMIT {
            return Err((413, "MCP body exceeds 8 MiB".into()));
        }
        body.resize(n, 0);
        reader.read_exact(&mut body).map_err(bad)?;
    } else if method == "POST" {
        return Err((411, "Content-Length or chunked framing required".into()));
    }
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}
fn handle(
    request: Request,
    port: u16,
    sender: &mpsc::SyncSender<Work>,
    alive: Arc<AtomicBool>,
    inflight: Inflight,
    stream: &TcpStream,
) -> Result<(u16, Value), HttpError> {
    let allowed = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if !request
        .headers
        .get("host")
        .is_some_and(|host| allowed.contains(host))
    {
        return Err((403, "invalid Host".into()));
    }
    if request
        .headers
        .get("origin")
        .is_some_and(|origin| !allowed.iter().any(|h| origin == &format!("http://{h}")))
    {
        return Err((403, "invalid Origin".into()));
    }
    if !matches!(request.path.as_str(), "/mcp" | "/") {
        return Err((404, "use POST /mcp".into()));
    }
    if request.method != "POST" {
        return Err((405, "use POST /mcp; no standalone SSE stream".into()));
    }
    if !request.headers.get("content-type").is_some_and(|v| {
        v.split(';')
            .next()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case("application/json")
    }) {
        return Err((415, "Content-Type must be application/json".into()));
    }
    let body: Value = match serde_json::from_slice(&request.body) {
        Ok(body) => body,
        Err(_) => {
            return Ok((
                400,
                protocol::error(Value::Null, -32700, "Parse error", Value::Null),
            ));
        }
    };
    protocol::dispatch(body, &request.headers, |method, params, id| {
        if method == "notifications/cancelled" {
            if let Some(id) = params.get("requestId")
                && let Some(flags) = inflight.lock().unwrap().get(&id.to_string())
                && flags.len() == 1
            {
                flags[0].store(true, Ordering::Release);
            }
            return Ok(Value::Null);
        }
        let key = id.to_string();
        let canceled = Arc::new(AtomicBool::new(false));
        {
            let mut pending = inflight.lock().unwrap();
            pending
                .entry(key.clone())
                .or_default()
                .push(Arc::clone(&canceled));
        }
        let (reply, receiver) = mpsc::sync_channel(1);
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let work = Work {
            method: method.into(),
            params,
            reply,
            alive: Arc::clone(&alive),
            canceled: Arc::clone(&canceled),
            deadline,
        };
        let result = if sender.try_send(work).is_err() {
            Err("application request queue is full".into())
        } else {
            let _ = stream.set_nonblocking(true);
            let result = loop {
                if !alive.load(Ordering::Acquire)
                    || canceled.load(Ordering::Acquire)
                    || Instant::now() >= deadline
                {
                    break Err(
                        "request canceled or expired; query current revision before retrying"
                            .into(),
                    );
                }
                match receiver.recv_timeout(Duration::from_millis(10)) {
                    Ok(result) => break result,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break Err("application closed".into());
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if matches!(stream.peek(&mut [0]), Ok(0)) {
                    break Err("client disconnected".into());
                }
            };
            let _ = stream.set_nonblocking(false);
            result
        };
        canceled.store(true, Ordering::Release);
        let mut pending = inflight.lock().unwrap();
        if let Some(flags) = pending.get_mut(&key) {
            flags.retain(|flag| !Arc::ptr_eq(flag, &canceled));
            if flags.is_empty() {
                pending.remove(&key);
            }
        }
        result
    })
    .map_err(bad)
}
fn respond(stream: &mut TcpStream, status: u16, value: Value) -> std::io::Result<()> {
    let body = if status == 202 {
        Vec::new()
    } else {
        serde_json::to_vec(&value)?
    };
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nAllow: POST\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(port: u16, body: &str, origin: Option<&str>, chunked: bool) -> String {
        let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let framing = if chunked {
            "Transfer-Encoding: chunked".into()
        } else {
            format!("Content-Length: {}", body.len())
        };
        write!(stream,"POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n{framing}\r\n{}\r\n",origin.map_or(String::new(),|o|format!("Origin: {o}\r\n"))).unwrap();
        if chunked {
            write!(stream, "{:x}\r\n{body}\r\n0\r\n\r\n", body.len()).unwrap();
        } else {
            stream.write_all(body.as_bytes()).unwrap();
        }
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }
    #[test]
    fn loopback_transport_handles_chunked_json_rejects_browser_origin_and_restarts_same_port() {
        let server = Server::start(0).unwrap();
        let port = server.port;
        let ping = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        for chunked in [false, true] {
            let response = request(port, ping, None, chunked);
            assert!(response.starts_with("HTTP/1.1 200"), "{response}");
            assert!(response.contains("\"result\":{}"));
        }
        assert!(
            request(port, ping, Some("https://example.com"), false).starts_with("HTTP/1.1 403")
        );
        drop(server);
        let restarted = Server::start(port).unwrap();
        assert!(request(restarted.port, ping, None, false).starts_with("HTTP/1.1 200"));
    }
    #[test]
    fn owner_queue_is_cancelled_on_disconnect_and_stop_releases_pending_calls() {
        let server = Server::start(0).unwrap();
        let port = server.port;
        let client = thread::spawn(move || {
            request(
                port,
                r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"k3d_status"}}"#,
                None,
                false,
            )
        });
        let start = Instant::now();
        let work = loop {
            if let Some(work) = server.try_recv() {
                break work;
            }
            assert!(start.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(5));
        };
        assert!(work.live());
        work.finish(Ok(json!({"revision":9})));
        assert!(client.join().unwrap().contains("\"revision\":9"));
        let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
        let body =
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"k3d_status"}}"#;
        write!(stream,"POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len()).unwrap();
        let start = Instant::now();
        let work = loop {
            if let Some(work) = server.try_recv() {
                break work;
            }
            assert!(start.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(5));
        };
        stream.shutdown(Shutdown::Both).unwrap();
        drop(stream);
        let start = Instant::now();
        while work.live() {
            assert!(start.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(5));
        }
        drop(server);
        assert!(!work.live());
    }
}

#[cfg(test)]
mod concurrent_tests {
    use super::*;
    #[test]
    fn independent_clients_can_reuse_the_same_json_rpc_counter() {
        let server = Server::start(0).unwrap();
        let port = server.port;
        let clients=(0..2).map(|_|thread::spawn(move||{
            let mut stream=TcpStream::connect((std::net::Ipv4Addr::LOCALHOST,port)).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let body=r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"k3d_status"}}"#;
            write!(stream,"POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len()).unwrap();
            let mut output=String::new();stream.read_to_string(&mut output).unwrap();output
        })).collect::<Vec<_>>();
        let start = Instant::now();
        let mut work = Vec::new();
        while work.len() < 2 {
            if let Some(request) = server.try_recv() {
                work.push(request);
            }
            assert!(start.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(5));
        }
        for request in work {
            assert!(request.live());
            request.finish(Ok(json!({"revision":1})));
        }
        for client in clients {
            let output = client.join().unwrap();
            assert!(output.contains("\"isError\":false"), "{output}");
        }
    }
}

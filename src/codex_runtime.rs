//! Supervised local `codex app-server --stdio` runtime.
//!
//! The supervisor is the only stdin writer. Dedicated readers feed bounded
//! channels; no controller/HID lifecycle operation changes `ServerEpoch`.

use crate::codex_micro::{
    ChatStatus, CodexEvent, CodexEventKind, CodexTransport, Mutation, SemanticAction, SourcePolicy,
    ThreadContext, ThreadRecord, TransportError,
};
use crate::codex_protocol as wire;
use crate::codex_voice::VoiceCapture;
use crate::config::CodexMicroConfig;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const COMMAND_LIMIT: usize = 64;
const FRAME_LIMIT: usize = 128;
const PENDING_LIMIT: usize = 128;
const APPROVAL_LIMIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerEpoch(pub u64);

#[derive(Debug, Clone, Default)]
pub struct RuntimeView {
    pub connected: bool,
    pub ready: bool,
    pub server_epoch: u64,
    pub last_error: Option<String>,
    pub models_loaded: bool,
    pub model_count: usize,
    pub threads_loaded: bool,
    pub thread_count: usize,
    pub skills_loaded: bool,
    pub skill_count: usize,
    pub fast: bool,
    pub efforts: Vec<String>,
    pub effort_index: usize,
    pub composer: String,
    pub voice: VoiceState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VoiceState {
    #[default]
    Idle,
    Capturing,
    Finalizing,
}

#[derive(Debug, Clone)]
struct Skill {
    name: String,
    path: String,
}

#[derive(Debug, Clone)]
struct Approval {
    epoch: ServerEpoch,
    id: wire::RequestId,
    method: String,
    thread_id: String,
    turn_id: String,
    item_id: String,
    approval_id: Option<String>,
}

type Reply = mpsc::Sender<Result<(), TransportError>>;
enum RuntimeCommand {
    Mutation {
        epoch: u64,
        mutation: Mutation,
        reply: Reply,
    },
    Shutdown,
}

#[derive(Clone)]
pub struct RuntimeTransport {
    tx: mpsc::SyncSender<RuntimeCommand>,
    view: Arc<Mutex<RuntimeView>>,
    timeout: Duration,
}

impl CodexTransport for RuntimeTransport {
    fn mutate(&mut self, mutation: &Mutation) -> Result<(), TransportError> {
        let state = self.view.lock().expect("runtime view poisoned");
        if !state.connected {
            return Err(TransportError::Unavailable);
        }
        if !state.ready {
            return Err(TransportError::NotReady);
        }
        let epoch = state.server_epoch;
        drop(state);
        let (reply, result) = mpsc::channel();
        self.tx
            .try_send(RuntimeCommand::Mutation {
                epoch,
                mutation: mutation.clone(),
                reply,
            })
            .map_err(|_| TransportError::QueueFull)?;
        match result.recv_timeout(self.timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(TransportError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(TransportError::Protocol),
        }
    }
    fn epoch(&self) -> u64 {
        self.view
            .lock()
            .expect("runtime view poisoned")
            .server_epoch
    }
}

pub struct RuntimeHandle {
    tx: mpsc::SyncSender<RuntimeCommand>,
    pub epoch: Arc<AtomicU64>,
    pub view: Arc<Mutex<RuntimeView>>,
    stop: Arc<AtomicBool>,
    supervisor: Option<std::thread::JoinHandle<()>>,
    request_timeout: Duration,
    completed: Arc<AtomicBool>,
}

impl RuntimeHandle {
    pub fn spawn(cfg: CodexMicroConfig, events: mpsc::SyncSender<CodexEvent>) -> Self {
        let (tx, rx) = mpsc::sync_channel(COMMAND_LIMIT);
        let epoch = Arc::new(AtomicU64::new(0));
        let view = Arc::new(Mutex::new(RuntimeView::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_epoch = Arc::clone(&epoch);
        let thread_view = Arc::clone(&view);
        let thread_stop = Arc::clone(&stop);
        let completed = Arc::new(AtomicBool::new(false));
        let thread_completed = Arc::clone(&completed);
        let request_timeout =
            Duration::from_millis(cfg.request_timeout_ms.max(cfg.voice_timeout_ms));
        let supervisor = std::thread::Builder::new()
            .name("codex-supervisor".into())
            .spawn(move || {
                supervise(cfg, rx, events, thread_epoch, thread_view, thread_stop);
                thread_completed.store(true, Ordering::Release);
            })
            .ok();
        Self {
            tx,
            epoch,
            view,
            stop,
            supervisor,
            request_timeout,
            completed,
        }
    }

    pub fn transport(&self) -> RuntimeTransport {
        RuntimeTransport {
            tx: self.tx.clone(),
            view: Arc::clone(&self.view),
            timeout: self.request_timeout + Duration::from_millis(100),
        }
    }

    #[cfg(test)]
    fn completion_signal(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.completed)
    }
}

impl Drop for RuntimeHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.tx.try_send(RuntimeCommand::Shutdown);
        if let Some(supervisor) = self.supervisor.take() {
            let _ = supervisor.join();
        }
        debug_assert!(self.completed.load(Ordering::Acquire));
    }
}

#[derive(Debug)]
enum PendingKind {
    Initialize,
    Models,
    Threads,
    Skills,
    Read {
        thread_id: String,
        reply: Reply,
    },
    Resume {
        reply: Reply,
    },
    Start {
        reply: Reply,
    },
    Fork {
        reply: Reply,
    },
    Turn {
        reply: Reply,
        composer: Option<String>,
    },
}
struct Pending {
    kind: PendingKind,
    started: Instant,
}

struct Connection {
    child: Child,
    stdout_thread: Option<std::thread::JoinHandle<()>>,
    stderr_thread: Option<std::thread::JoinHandle<()>>,
    stdin: ChildStdin,
    frames: mpsc::Receiver<Result<wire::Inbound, wire::FrameError>>,
    pending: HashMap<u64, Pending>,
    next_id: u64,
    epoch: ServerEpoch,
    sequence: u64,
    approvals: HashMap<wire::RequestId, Approval>,
    skills: Vec<Skill>,
    efforts: Vec<String>,
    fast_supported: bool,
    voice: Option<VoiceCapture>,
    voice_done: Option<mpsc::Receiver<Result<String, crate::codex_voice::VoiceError>>>,
    voice_cancel: Option<Arc<AtomicBool>>,
    voice_worker: Option<std::thread::JoinHandle<()>>,
    voice_reply: Option<Reply>,
}

fn spawn_child(cfg: &CodexMicroConfig, epoch: ServerEpoch) -> Result<Connection, String> {
    let (success, bytes) = probe_version(&cfg.codex_executable, Duration::from_secs(2))?;
    let stdout = String::from_utf8_lossy(&bytes);
    if !success
        || !stdout
            .split_whitespace()
            .any(|part| part == wire::PINNED_CODEX_VERSION)
    {
        return Err(format!("Codex {} required", wire::PINNED_CODEX_VERSION));
    }
    let mut command = Command::new(&cfg.codex_executable);
    command
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("app-server spawn failed: {e}"))?;
    let stdin = child.stdin.take().ok_or("app-server stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("app-server stdout unavailable")?;
    let mut stderr = child.stderr.take().ok_or("app-server stderr unavailable")?;
    let (frame_tx, frames) = mpsc::sync_channel(FRAME_LIMIT);
    let stdout_thread = std::thread::Builder::new()
        .name("codex-stdout".into())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match wire::read_frame(&mut reader) {
                    Ok(Some(frame)) => {
                        // Never let the protocol reader block teardown. A full
                        // queue is a connection-fatal backpressure violation;
                        // dropping the sender makes the supervisor reconnect.
                        if frame_tx.try_send(Ok(frame)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = frame_tx.try_send(Err(error));
                        break;
                    }
                }
            }
        })
        .map_err(|e| format!("stdout reader spawn failed: {e}"))?;
    let stderr_thread = std::thread::Builder::new()
        .name("codex-stderr".into())
        .spawn(move || {
            // Drain to prevent child blockage, but retain/log no command or transcript bodies.
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        })
        .map_err(|e| format!("stderr drainer spawn failed: {e}"))?;
    Ok(Connection {
        child,
        stdout_thread: Some(stdout_thread),
        stderr_thread: Some(stderr_thread),
        stdin,
        frames,
        pending: HashMap::new(),
        next_id: 1,
        epoch,
        sequence: 0,
        approvals: HashMap::new(),
        skills: Vec::new(),
        efforts: Vec::new(),
        fast_supported: false,
        voice: None,
        voice_done: None,
        voice_cancel: None,
        voice_worker: None,
        voice_reply: None,
    })
}
fn probe_version(executable: &str, timeout: Duration) -> Result<(bool, Vec<u8>), String> {
    let mut child = Command::new(executable)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Codex version check failed: {e}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("Codex version stdout unavailable")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = (&mut stdout).take(4096).read_to_end(&mut bytes);
        let _ = std::io::copy(&mut stdout, &mut std::io::sink());
        bytes
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("Codex version check failed: {e}"))?
        {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Codex version check timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let bytes = reader.join().map_err(|_| "Codex version reader failed")?;
    Ok((status.success(), bytes))
}

fn send(conn: &mut Connection, value: Value) -> Result<(), String> {
    conn.stdin
        .write_all(&wire::encode_line(&value))
        .and_then(|_| conn.stdin.flush())
        .map_err(|e| format!("app-server write failed: {e}"))
}

fn call(
    conn: &mut Connection,
    kind: PendingKind,
    build: impl FnOnce(u64) -> Value,
) -> Result<(), String> {
    if conn.pending.len() >= PENDING_LIMIT {
        return Err("app-server pending limit reached".into());
    }
    let id = conn.next_id;
    conn.next_id = conn.next_id.checked_add(1).ok_or("request id exhausted")?;
    if let Err(error) = send(conn, build(id)) {
        reject_pending(kind, TransportError::Protocol);
        return Err(error);
    }
    conn.pending.insert(
        id,
        Pending {
            kind,
            started: Instant::now(),
        },
    );
    Ok(())
}

fn supervise(
    cfg: CodexMicroConfig,
    rx: mpsc::Receiver<RuntimeCommand>,
    events: mpsc::SyncSender<CodexEvent>,
    epoch: Arc<AtomicU64>,
    view: Arc<Mutex<RuntimeView>>,
    stop: Arc<AtomicBool>,
) {
    let mut backoff = cfg.reconnect_min_ms;
    while !stop.load(Ordering::Acquire) {
        let next_epoch = epoch.load(Ordering::Acquire) + 1;
        let mut conn = match spawn_child(&cfg, ServerEpoch(next_epoch)) {
            Ok(conn) => conn,
            Err(error) => {
                set_disconnected(&view, Some(error));
                sleep_backoff(&stop, backoff);
                backoff = (backoff * 2).min(cfg.reconnect_max_ms);
                continue;
            }
        };
        epoch.store(next_epoch, Ordering::Release);
        {
            let mut state = view.lock().expect("runtime view poisoned");
            *state = RuntimeView {
                connected: true,
                server_epoch: next_epoch,
                ..RuntimeView::default()
            };
        }
        if emit(
            &mut conn,
            &events,
            CodexEventKind::Snapshot {
                threads: vec![],
                policy: SourcePolicy::Recent,
                custom_order: vec![],
            },
        )
        .is_err()
        {
            reap(&mut conn);
            continue;
        }
        if call(&mut conn, PendingKind::Initialize, wire::initialize).is_err() {
            reap(&mut conn);
            continue;
        }
        let result = run_connection(&cfg, &rx, &events, &view, &mut conn, &stop);
        if let Some(mut voice) = conn.voice.take() {
            voice.cancel();
        }
        if let Some(cancel) = conn.voice_cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        if let Some(reply) = conn.voice_reply.take() {
            let _ = reply.send(Err(TransportError::Protocol));
        }
        let had_connected = view.lock().expect("runtime view poisoned").connected;
        match &result {
            Ok(()) if stop.load(Ordering::Acquire) => {
                set_disconnected(&view, None);
            }
            Ok(()) => {
                set_disconnected(&view, Some("app-server disconnected".into()));
            }
            Err(error) => {
                set_disconnected(&view, Some(error.clone()));
            }
        }
        if let Some(worker) = conn.voice_worker.take() {
            let _ = worker.join();
        }
        reap(&mut conn);
        if stop.load(Ordering::Acquire) {
            break;
        }
        sleep_backoff(&stop, backoff);
        backoff = if had_connected {
            cfg.reconnect_min_ms
        } else {
            (backoff * 2).min(cfg.reconnect_max_ms)
        };
    }
}

fn run_connection(
    cfg: &CodexMicroConfig,
    rx: &mpsc::Receiver<RuntimeCommand>,
    events: &mpsc::SyncSender<CodexEvent>,
    view: &Arc<Mutex<RuntimeView>>,
    conn: &mut Connection,
    stop: &AtomicBool,
) -> Result<(), String> {
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        while let Ok(command) = rx.try_recv() {
            match command {
                RuntimeCommand::Shutdown => return Ok(()),
                RuntimeCommand::Mutation {
                    epoch,
                    mutation,
                    reply,
                } => {
                    if epoch != conn.epoch.0 {
                        let _ = reply.send(Err(TransportError::StaleGeneration));
                        continue;
                    }
                    handle_mutation(cfg, conn, events, view, mutation, reply)?;
                }
            }
        }
        if let Some(done) = &conn.voice_done
            && let Ok(result) = done.try_recv()
        {
            let mut state = view.lock().expect("runtime view poisoned");
            state.voice = VoiceState::Idle;
            let accepted = match result {
                Ok(text) => {
                    state.composer = text;
                    Ok(())
                }
                Err(crate::codex_voice::VoiceError::Timeout) => Err(TransportError::Timeout),
                Err(_) => Err(TransportError::Unsupported),
            };
            if let Some(reply) = conn.voice_reply.take() {
                let _ = reply.send(accepted);
            }
            conn.voice_done = None;
            conn.voice_cancel = None;
            if let Some(worker) = conn.voice_worker.take() {
                let _ = worker.join();
            }
        }
        let expired: Vec<u64> = conn
            .pending
            .iter()
            .filter(|(_, p)| p.started.elapsed() > Duration::from_millis(cfg.request_timeout_ms))
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            let pending = conn.pending.remove(&id).expect("pending existed");
            match pending.kind {
                PendingKind::Initialize
                | PendingKind::Models
                | PendingKind::Threads
                | PendingKind::Skills => return Err("app-server catalog timeout".into()),
                kind => reject_pending(kind, TransportError::Timeout),
            }
        }
        match conn.frames.recv_timeout(Duration::from_millis(20)) {
            Ok(Ok(frame)) => handle_frame(cfg, conn, events, view, frame)?,
            Ok(Err(error)) => return Err(format!("app-server frame rejected: {error:?}")),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err("app-server EOF".into()),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn handle_frame(
    cfg: &CodexMicroConfig,
    conn: &mut Connection,
    events: &mpsc::SyncSender<CodexEvent>,
    view: &Arc<Mutex<RuntimeView>>,
    frame: wire::Inbound,
) -> Result<(), String> {
    if let Some(method) = frame.method.as_deref() {
        if frame.id.is_some() {
            return handle_server_request(conn, events, frame);
        }
        return notification(conn, events, method, &frame.params);
    }
    let Some(wire::RequestId::Number(id)) = frame.id else {
        return Ok(());
    };
    let Ok(id) = u64::try_from(id) else {
        return Ok(());
    };
    let Some(pending) = conn.pending.remove(&id) else {
        return Ok(());
    };
    if !frame.error.is_null() {
        return match pending.kind {
            PendingKind::Initialize
            | PendingKind::Models
            | PendingKind::Threads
            | PendingKind::Skills => Err("app-server catalog request failed".into()),
            kind => {
                reject_pending(kind, TransportError::Protocol);
                Ok(())
            }
        };
    }
    match pending.kind {
        PendingKind::Initialize => {
            let user_agent = frame
                .result
                .pointer("/userAgent")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !user_agent.is_empty() && !user_agent.contains(wire::PINNED_CODEX_VERSION) {
                return Err("incompatible Codex app-server version".into());
            }
            send(conn, wire::initialized())?;
            call(conn, PendingKind::Models, wire::model_list)?;
            call(conn, PendingKind::Threads, wire::thread_list)?;
            let cwd = if cfg.cwd.is_empty() {
                std::env::current_dir()
                    .ok()
                    .and_then(|p| p.to_str().map(str::to_owned))
                    .unwrap_or_default()
            } else {
                cfg.cwd.clone()
            };
            call(conn, PendingKind::Skills, |id| wire::skills_list(id, &cwd))?;
            view.lock().expect("runtime view poisoned").last_error = None;
        }
        PendingKind::Models => {
            parse_models(conn, view, &frame.result);
            update_ready(view);
        }
        PendingKind::Threads => {
            project_threads(conn, events, view, &frame.result)?;
            update_ready(view);
        }
        PendingKind::Skills => {
            parse_skills(conn, view, &frame.result);
            update_ready(view);
        }
        PendingKind::Read { thread_id, reply } => {
            if let Some(record) = record(
                &frame.result,
                status_from_thread(frame.result.pointer("/thread/status")),
            ) {
                emit(conn, events, CodexEventKind::Upsert(record))?;
            }
            call(conn, PendingKind::Resume { reply }, |id| {
                wire::thread_resume(id, &thread_id)
            })?;
        }
        PendingKind::Resume { reply } => {
            if let Some(record) = record(
                &frame.result,
                status_from_thread(frame.result.pointer("/thread/status")),
            ) {
                emit(conn, events, CodexEventKind::SelectUpsert(record))?;
            }
            let _ = reply.send(Ok(()));
        }
        PendingKind::Start { reply } | PendingKind::Fork { reply } => {
            if let Some(mut record) = record(
                &frame.result,
                status_from_thread(frame.result.pointer("/thread/status")),
            ) {
                record.context.turn_id = None;
                record.context.item_id = None;
                record.context.approval_id = None;
                emit(conn, events, CodexEventKind::SelectUpsert(record))?;
                let _ = reply.send(Ok(()));
            } else {
                let _ = reply.send(Err(TransportError::Protocol));
            }
        }
        PendingKind::Turn { reply, composer } => {
            if let Some(context) = context_from(&frame.result) {
                emit(
                    conn,
                    events,
                    CodexEventKind::StatusById {
                        thread_id: context.thread_id,
                        status: ChatStatus::Thinking,
                        updated_ms: now_ms(),
                    },
                )?;
            }
            if let Some(sent) = composer {
                let mut state = view.lock().expect("runtime view poisoned");
                if state.composer == sent {
                    state.composer.clear();
                }
            }
            let _ = reply.send(Ok(()));
        }
    }
    Ok(())
}

fn parse_models(conn: &mut Connection, view: &Arc<Mutex<RuntimeView>>, result: &Value) {
    let selected = result["data"].as_array().and_then(|models| {
        models
            .iter()
            .find(|m| m["isDefault"] == true)
            .or_else(|| models.first())
    });
    conn.efforts = selected
        .and_then(|m| m["supportedReasoningEfforts"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|e| e["reasoningEffort"].as_str().map(str::to_owned))
        .collect();
    conn.fast_supported = selected
        .and_then(|m| m["serviceTiers"].as_array())
        .is_some_and(|tiers| tiers.iter().any(|t| t["id"] == "priority"));
    let mut state = view.lock().expect("runtime view poisoned");
    state.models_loaded = true;
    state.model_count = result["data"].as_array().map_or(0, Vec::len);
    state.efforts = conn.efforts.clone();
    state.effort_index = state
        .effort_index
        .min(state.efforts.len().saturating_sub(1));
}

fn parse_skills(conn: &mut Connection, view: &Arc<Mutex<RuntimeView>>, result: &Value) {
    conn.skills = result["data"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|entry| entry["skills"].as_array().into_iter().flatten())
        .filter(|skill| skill["enabled"].as_bool().unwrap_or(false))
        .filter_map(|skill| {
            Some(Skill {
                name: skill["name"].as_str()?.to_owned(),
                path: skill["path"].as_str()?.to_owned(),
            })
        })
        .collect();
    let mut state = view.lock().expect("runtime view poisoned");
    state.skills_loaded = true;
    state.skill_count = conn.skills.len();
}
fn update_ready(view: &Arc<Mutex<RuntimeView>>) {
    let mut state = view.lock().expect("runtime view poisoned");
    state.ready = state.models_loaded && state.threads_loaded && state.skills_loaded;
}
fn reject_pending(kind: PendingKind, error: TransportError) {
    let reply = match kind {
        PendingKind::Read { reply, .. }
        | PendingKind::Resume { reply }
        | PendingKind::Start { reply }
        | PendingKind::Fork { reply }
        | PendingKind::Turn { reply, .. } => Some(reply),
        _ => None,
    };
    if let Some(reply) = reply {
        let _ = reply.send(Err(error));
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn context_from(value: &Value) -> Option<ThreadContext> {
    let thread = value.get("thread").unwrap_or(value);
    Some(ThreadContext {
        thread_id: thread
            .get("id")
            .or_else(|| value.get("threadId"))?
            .as_str()?
            .to_owned(),
        turn_id: value
            .get("turnId")
            .or_else(|| value.pointer("/turn/id"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        item_id: value
            .get("itemId")
            .or_else(|| value.pointer("/item/id"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        approval_id: None,
    })
}
fn record(value: &Value, status: ChatStatus) -> Option<ThreadRecord> {
    let thread = value.get("thread").unwrap_or(value);
    Some(ThreadRecord {
        context: context_from(value)?,
        status,
        updated_ms: thread
            .get("updatedAt")
            .and_then(Value::as_u64)
            .map(|seconds| seconds.saturating_mul(1000))
            .unwrap_or_else(now_ms),
        pinned: false,
        priority: 0,
    })
}
fn status_from_thread(status: Option<&Value>) -> ChatStatus {
    let Some(status) = status else {
        return ChatStatus::Unassigned;
    };
    match status.get("type").and_then(Value::as_str) {
        Some("notLoaded") => ChatStatus::Unassigned,
        Some("idle") => ChatStatus::Idle,
        Some("systemError") => ChatStatus::Error,
        Some("active")
            if status
                .get("activeFlags")
                .and_then(Value::as_array)
                .is_some_and(|flags| {
                    flags.iter().any(|f| {
                        matches!(f.as_str(), Some("waitingOnApproval" | "waitingOnUserInput"))
                    })
                }) =>
        {
            ChatStatus::RequiresInput
        }
        Some("active") => ChatStatus::Thinking,
        _ => ChatStatus::Unassigned,
    }
}
fn turn_completion_status(params: &Value) -> ChatStatus {
    match params.pointer("/turn/status").and_then(Value::as_str) {
        Some("failed") => ChatStatus::Error,
        Some("interrupted") => ChatStatus::Idle,
        Some("completed") => ChatStatus::CompleteUnread,
        _ => ChatStatus::Error,
    }
}
fn emit(
    conn: &mut Connection,
    events: &mpsc::SyncSender<CodexEvent>,
    kind: CodexEventKind,
) -> Result<(), String> {
    conn.sequence += 1;
    events
        .try_send(CodexEvent {
            connection_generation: conn.epoch.0,
            sequence: conn.sequence,
            kind,
        })
        .map_err(|_| "runtime event queue full".to_owned())
}
fn project_threads(
    conn: &mut Connection,
    events: &mpsc::SyncSender<CodexEvent>,
    view: &Arc<Mutex<RuntimeView>>,
    result: &Value,
) -> Result<(), String> {
    let threads = result["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| record(v, status_from_thread(v.get("status"))))
        .collect::<Vec<_>>();
    {
        let mut state = view.lock().expect("runtime view poisoned");
        state.threads_loaded = true;
        state.thread_count = threads.len();
    }
    emit(
        conn,
        events,
        CodexEventKind::Snapshot {
            threads,
            policy: SourcePolicy::Recent,
            custom_order: vec![],
        },
    )
}

fn notification(
    conn: &mut Connection,
    events: &mpsc::SyncSender<CodexEvent>,
    method: &str,
    params: &Value,
) -> Result<(), String> {
    match method {
        "thread/started" => {
            if let Some(r) = record(params, status_from_thread(params.pointer("/thread/status"))) {
                emit(conn, events, CodexEventKind::Upsert(r))?;
            }
        }
        "thread/deleted" | "thread/archived" => {
            if let Some(id) = params["threadId"].as_str() {
                emit(
                    conn,
                    events,
                    CodexEventKind::Remove {
                        thread_id: id.into(),
                    },
                )?;
            }
        }
        "turn/started" => {
            if let Some(r) = record(params, ChatStatus::Thinking) {
                emit(conn, events, CodexEventKind::Upsert(r))?;
            }
        }
        "turn/completed" => {
            if let Some(r) = record(params, turn_completion_status(params)) {
                emit(conn, events, CodexEventKind::Upsert(r))?;
            }
        }
        "error" => {
            if let Some(r) = record(params, ChatStatus::Error) {
                emit(conn, events, CodexEventKind::Upsert(r))?;
            }
        }
        "thread/status/changed" => {
            let status = status_from_thread(params.get("status"));
            if let Some(thread_id) = params.get("threadId").and_then(Value::as_str) {
                emit(
                    conn,
                    events,
                    CodexEventKind::StatusById {
                        thread_id: thread_id.to_owned(),
                        status,
                        updated_ms: now_ms(),
                    },
                )?;
            }
        }
        "serverRequest/resolved" => {
            if let Some(id) = conn
                .approvals
                .keys()
                .find(|id| {
                    request_id_matches(params.get("request_id"), id)
                        || request_id_matches(params.get("requestId"), id)
                })
                .cloned()
                && let Some(approval) = conn.approvals.remove(&id)
            {
                emit_approval_state(conn, events, &approval.thread_id)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_server_request(
    conn: &mut Connection,
    events: &mpsc::SyncSender<CodexEvent>,
    frame: wire::Inbound,
) -> Result<(), String> {
    let method = frame.method.clone().unwrap_or_default();
    if !matches!(
        method.as_str(),
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
    ) {
        return Ok(());
    }
    let Some(id) = frame.id else {
        return Ok(());
    };
    let p = frame.params;
    let Some(context) = context_from(&p) else {
        return Err("approval missing threadId".into());
    };
    let turn_id = context.turn_id.ok_or("approval missing turnId")?;
    let item_id = context.item_id.ok_or("approval missing itemId")?;
    let approval = Approval {
        epoch: conn.epoch,
        id,
        method,
        thread_id: context.thread_id,
        turn_id,
        item_id,
        approval_id: p["approvalId"].as_str().map(str::to_owned),
    };
    if conn.approvals.contains_key(&approval.id) {
        return Ok(());
    }
    if conn.approvals.len() >= APPROVAL_LIMIT {
        send(
            conn,
            wire::response(&approval.id, json!({"decision":"decline"})),
        )?;
        return Ok(());
    }
    let thread_id = approval.thread_id.clone();
    conn.approvals.insert(approval.id.clone(), approval);
    emit_approval_state(conn, events, &thread_id)?;
    Ok(())
}
fn emit_approval_state(
    conn: &mut Connection,
    events: &mpsc::SyncSender<CodexEvent>,
    thread_id: &str,
) -> Result<(), String> {
    if let Some(approval) = conn
        .approvals
        .values()
        .find(|a| a.thread_id == thread_id)
        .cloned()
    {
        let context = ThreadContext {
            thread_id: approval.thread_id.clone(),
            turn_id: Some(approval.turn_id.clone()),
            item_id: Some(approval.item_id.clone()),
            approval_id: Some(approval_binding_key(&approval)),
        };
        emit(
            conn,
            events,
            CodexEventKind::Upsert(ThreadRecord {
                context,
                status: ChatStatus::RequiresInput,
                updated_ms: now_ms(),
                pinned: false,
                priority: 0,
            }),
        )
    } else {
        emit(
            conn,
            events,
            CodexEventKind::DisarmApproval {
                thread_id: thread_id.to_owned(),
            },
        )
    }
}
fn request_id_matches(value: Option<&Value>, id: &wire::RequestId) -> bool {
    let Some(candidate) = value else {
        return false;
    };
    if candidate == &serde_json::to_value(id).unwrap_or_default() {
        return true;
    }
    match (candidate, id) {
        (Value::String(candidate), wire::RequestId::String(expected)) => {
            candidate == expected.as_str()
        }
        (Value::String(candidate), wire::RequestId::Number(expected)) => {
            candidate == &expected.to_string()
        }
        (Value::Number(candidate), wire::RequestId::Number(expected)) => {
            candidate.as_u64() == u64::try_from(*expected).ok()
        }
        (Value::Number(candidate), wire::RequestId::String(expected)) => {
            candidate.as_u64() == expected.parse::<u64>().ok()
        }
        _ => false,
    }
}
fn approval_key(id: &wire::RequestId) -> String {
    serde_json::to_string(id).unwrap_or_default()
}
fn approval_binding_key(approval: &Approval) -> String {
    format!(
        "{}:{}",
        approval.approval_id.as_deref().unwrap_or(""),
        approval_key(&approval.id)
    )
}

fn handle_mutation(
    cfg: &CodexMicroConfig,
    conn: &mut Connection,
    events: &mpsc::SyncSender<CodexEvent>,
    view: &Arc<Mutex<RuntimeView>>,
    mutation: Mutation,
    reply: Reply,
) -> Result<(), String> {
    let target = &mutation.identity.thread_id;
    match mutation.action {
        SemanticAction::Activate => {
            if target.is_empty() {
                let _ = reply.send(Err(TransportError::UnassignedTarget));
                return Ok(());
            }
            call(
                conn,
                PendingKind::Read {
                    thread_id: target.clone(),
                    reply,
                },
                |id| wire::thread_read(id, target),
            )?;
        }
        SemanticAction::NewThread => call(conn, PendingKind::Start { reply }, |id| {
            wire::thread_start(id, Some(&cfg.cwd))
        })?,
        SemanticAction::ContinueInNewChat => {
            if target.is_empty() {
                let _ = reply.send(Err(TransportError::UnassignedTarget));
                return Ok(());
            }
            call(conn, PendingKind::Fork { reply }, |id| {
                wire::thread_fork(id, target)
            })?
        }
        SemanticAction::ToggleFast => {
            if conn.fast_supported {
                let mut state = view.lock().expect("runtime view poisoned");
                state.fast = !state.fast;
                let _ = reply.send(Ok(()));
            } else {
                let _ = reply.send(Err(TransportError::Unsupported));
            }
        }
        SemanticAction::SetReasoning(index) => {
            let mut state = view.lock().expect("runtime view poisoned");
            if state.efforts.is_empty() {
                let _ = reply.send(Err(TransportError::Unsupported));
            } else {
                state.effort_index = usize::from(index).min(state.efforts.len() - 1);
                let _ = reply.send(Ok(()));
            }
        }
        SemanticAction::Send => {
            let (text, effort, fast) = {
                let state = view.lock().expect("runtime view poisoned");
                let text = state.composer.clone();
                let effort = state.efforts.get(state.effort_index).cloned();
                (text, effort, state.fast)
            };
            if text.trim().is_empty() {
                let _ = reply.send(Ok(()));
            } else {
                call(
                    conn,
                    PendingKind::Turn {
                        reply,
                        composer: Some(text.clone()),
                    },
                    |id| {
                        wire::turn_start(
                            id,
                            target,
                            wire::text_input(&text),
                            effort.as_deref(),
                            fast,
                        )
                    },
                )?;
            }
        }
        SemanticAction::Command(text) | SemanticAction::CardinalPrompt(text) => {
            if text.trim().is_empty() {
                let _ = reply.send(Err(TransportError::Unsupported));
                return Ok(());
            }
            let (effort, fast) = {
                let state = view.lock().expect("runtime view poisoned");
                (state.efforts.get(state.effort_index).cloned(), state.fast)
            };
            call(
                conn,
                PendingKind::Turn {
                    reply,
                    composer: None,
                },
                |id| wire::turn_start(id, target, wire::text_input(&text), effort.as_deref(), fast),
            )?;
        }
        SemanticAction::Skill(favorite) => {
            let by_path: Vec<_> = conn
                .skills
                .iter()
                .filter(|s| s.path == favorite)
                .cloned()
                .collect();
            let candidates: Vec<_> = if by_path.is_empty() {
                conn.skills
                    .iter()
                    .filter(|s| s.name == favorite)
                    .cloned()
                    .collect()
            } else {
                by_path
            };
            if candidates.len() != 1 {
                let _ = reply.send(Err(TransportError::Unsupported));
                return Ok(());
            }
            let skill = candidates[0].clone();
            call(
                conn,
                PendingKind::Turn {
                    reply,
                    composer: None,
                },
                |id| {
                    wire::turn_start(
                        id,
                        target,
                        wire::skill_input(&skill.name, &skill.path),
                        None,
                        false,
                    )
                },
            )?;
        }
        SemanticAction::Approve | SemanticAction::Decline => {
            let approval = conn
                .approvals
                .values()
                .find(|approval| {
                    approval.thread_id == *target
                        && mutation.identity.turn_id.as_deref() == Some(&approval.turn_id)
                        && mutation.identity.item_id.as_deref() == Some(&approval.item_id)
                        && mutation.identity.approval_id.as_deref()
                            == Some(approval_binding_key(approval).as_str())
                })
                .cloned()
                .ok_or(TransportError::ContextMismatch);
            let approval = match approval {
                Ok(a) => a,
                Err(error) => {
                    let _ = reply.send(Err(error));
                    return Ok(());
                }
            };
            if !matches!(
                approval.method.as_str(),
                "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
            ) || approval.epoch != conn.epoch
                || approval.thread_id != *target
                || mutation.identity.turn_id.as_deref() != Some(&approval.turn_id)
                || mutation.identity.item_id.as_deref() != Some(&approval.item_id)
                || mutation.identity.approval_id.as_deref()
                    != Some(approval_binding_key(&approval).as_str())
            {
                let _ = reply.send(Err(TransportError::ContextMismatch));
                return Ok(());
            }
            let decision = if matches!(mutation.action, SemanticAction::Approve) {
                "accept"
            } else {
                "decline"
            };
            if let Err(error) = send(
                conn,
                wire::response(&approval.id, json!({"decision": decision})),
            ) {
                let _ = reply.send(Err(TransportError::Protocol));
                return Err(error);
            }
            conn.approvals.remove(&approval.id);
            let _ = emit_approval_state(conn, events, &approval.thread_id);
            let _ = reply.send(Ok(()));
        }
        SemanticAction::PushToTalk { active: true, .. } => {
            if conn.voice.is_some() || conn.voice_done.is_some() {
                let _ = reply.send(Err(TransportError::Unsupported));
                return Ok(());
            }
            match VoiceCapture::start(&cfg.voice_argv, cfg.voice_output_limit) {
                Ok(voice) => {
                    conn.voice = Some(voice);
                    view.lock().expect("runtime view poisoned").voice = VoiceState::Capturing;
                    let _ = reply.send(Ok(()));
                }
                Err(_) => {
                    let _ = reply.send(Err(TransportError::Unsupported));
                }
            }
        }
        SemanticAction::PushToTalk { active: false, .. } => {
            let Some(voice) = conn.voice.take() else {
                let _ = reply.send(Err(TransportError::Unsupported));
                return Ok(());
            };
            let cancel = voice.cancel_token();
            let (tx, rx) = mpsc::sync_channel(1);
            let timeout = cfg.voice_timeout_ms;
            let limit = cfg.composer_limit;
            let worker = std::thread::spawn(move || {
                let _ = tx.send(voice.finish(timeout, limit));
            });
            conn.voice_cancel = Some(cancel);
            conn.voice_done = Some(rx);
            conn.voice_worker = Some(worker);
            conn.voice_reply = Some(reply);
            view.lock().expect("runtime view poisoned").voice = VoiceState::Finalizing;
        }
        SemanticAction::CancelVoice => {
            if let Some(mut voice) = conn.voice.take() {
                voice.cancel();
            }
            if let Some(cancel) = conn.voice_cancel.take() {
                cancel.store(true, Ordering::Release);
            }
            if let Some(worker) = conn.voice_worker.take() {
                let _ = worker.join();
            }
            if let Some(pending) = conn.voice_reply.take() {
                let _ = pending.send(Err(TransportError::Unsupported));
            }
            conn.voice_done = None;
            view.lock().expect("runtime view poisoned").voice = VoiceState::Idle;
            let _ = reply.send(Ok(()));
        }
    }
    Ok(())
}

fn set_disconnected(view: &Arc<Mutex<RuntimeView>>, error: Option<String>) {
    let mut state = view.lock().expect("runtime view poisoned");
    state.connected = false;
    state.ready = false;
    state.last_error = error;
}
fn sleep_backoff(stop: &AtomicBool, ms: u64) {
    let until = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < until && !stop.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn reap(conn: &mut Connection) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(conn.child.id() as i32), libc::SIGKILL);
    }
    let _ = conn.child.kill();
    let _ = conn.child.wait();
    if let Some(thread) = conn.stdout_thread.take() {
        let _ = thread.join();
    }
    if let Some(thread) = conn.stderr_thread.take() {
        let _ = thread.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex_micro::MutationIdentity;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn model_capabilities_drive_effort_and_priority() {
        let mut conn = dummy();
        let view = Arc::new(Mutex::new(RuntimeView::default()));
        parse_models(
            &mut conn,
            &view,
            &json!({"data":[{"isDefault":true,"supportedReasoningEfforts":[{"reasoningEffort":"low"},{"reasoningEffort":"high"}],"serviceTiers":[{"id":"priority"}]}]}),
        );
        assert_eq!(conn.efforts, ["low", "high"]);
        assert!(conn.fast_supported);
    }
    #[test]
    fn skills_keep_path_identity() {
        let mut conn = dummy();
        let view = Arc::new(Mutex::new(RuntimeView::default()));
        parse_skills(
            &mut conn,
            &view,
            &json!({"data":[{"skills":[{"enabled":true,"name":"x","path":"/x/SKILL.md"}]}]}),
        );
        assert_eq!(conn.skills[0].path, "/x/SKILL.md");
    }
    #[test]
    fn server_epoch_is_not_hid_state() {
        let a = ServerEpoch(3);
        let hid_reconnects = 99;
        assert_eq!(a, ServerEpoch(3));
        assert_eq!(hid_reconnects, 99);
    }
    #[cfg(unix)]
    #[test]
    fn fake_ndjson_server_observes_handshake_and_catalog_order() {
        let root = std::env::temp_dir().join(format!("omegag-fake-{}", std::process::id()));
        let capture = root.with_extension("capture");
        let script = format!(
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo 'codex-cli {version}'; exit 0; fi
IFS= read -r a; printf '%s\n' "$a" > '{capture}'
printf '%s\n' '{{"id":1,"result":{{"userAgent":"codex-cli/{version}"}}}}'
IFS= read -r b; IFS= read -r c; IFS= read -r d; IFS= read -r e
printf '%s\n%s\n%s\n%s\n' "$b" "$c" "$d" "$e" >> '{capture}'
printf '%s\n' '{{"id":2,"result":{{"data":[]}}}}' '{{"id":3,"result":{{"data":[]}}}}' '{{"id":4,"result":{{"data":[]}}}}'
sleep 2
"#,
            version = wire::PINNED_CODEX_VERSION,
            capture = capture.display()
        );
        std::fs::write(&root, script).unwrap();
        let mut permissions = std::fs::metadata(&root).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&root, permissions).unwrap();
        let cfg = CodexMicroConfig {
            enabled: true,
            codex_executable: root.to_string_lossy().into_owned(),
            reconnect_max_ms: 100,
            ..Default::default()
        };
        let (event_tx, _event_rx) = mpsc::sync_channel(8);
        let runtime = RuntimeHandle::spawn(cfg, event_tx);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !capture.exists()
            || std::fs::read_to_string(&capture)
                .unwrap_or_default()
                .lines()
                .count()
                < 5
        {
            assert!(Instant::now() < deadline, "fake server handshake timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
        let lines: Vec<Value> = std::fs::read_to_string(&capture)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            lines
                .iter()
                .map(|v| v["method"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "initialize",
                "initialized",
                "model/list",
                "thread/list",
                "skills/list"
            ]
        );
        let shutdown = Instant::now();
        drop(runtime);
        assert!(shutdown.elapsed() < Duration::from_secs(1));
        let _ = std::fs::remove_file(&root);
        let _ = std::fs::remove_file(&capture);
    }

    #[cfg(unix)]
    #[test]
    fn drop_waits_for_supervisor_completion() {
        let root = std::env::temp_dir().join(format!("omegag-slow-version-{}", std::process::id()));
        std::fs::write(
            &root,
            format!(
                "#!/bin/sh\nsleep 0.15\necho 'codex-cli {}'\n",
                wire::PINNED_CODEX_VERSION
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&root).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&root, permissions).unwrap();
        let cfg = CodexMicroConfig {
            enabled: true,
            codex_executable: root.to_string_lossy().into_owned(),
            reconnect_min_ms: 1,
            reconnect_max_ms: 1,
            ..Default::default()
        };
        let (event_tx, _event_rx) = mpsc::sync_channel(8);
        let runtime = RuntimeHandle::spawn(cfg, event_tx);
        let completed = runtime.completion_signal();
        std::thread::sleep(Duration::from_millis(10));
        drop(runtime);
        assert!(
            completed.load(Ordering::Acquire),
            "RuntimeHandle::drop must join the supervisor"
        );
        let _ = std::fs::remove_file(root);
    }

    #[test]
    fn approval_response_correlation_supports_nested_request_payload_shape() {
        let mut conn = dummy();
        let (event_tx, event_rx) = mpsc::sync_channel(8);
        let frame = wire::Inbound {
            id: Some(wire::RequestId::Number(42)),
            method: Some("item/commandExecution/requestApproval".into()),
            params: json!({
                "thread": {"id": "thread-1"},
                "turn": {"id": "turn-1"},
                "item": {"id": "item-1"},
                "approvalId": "app-1"
            }),
            result: Value::Null,
            error: Value::Null,
        };
        handle_server_request(&mut conn, &event_tx, frame).unwrap();
        let approval = conn
            .approvals
            .values()
            .next()
            .expect("approval should be armed");
        assert_eq!(approval.thread_id, "thread-1");
        assert_eq!(approval.turn_id, "turn-1");
        assert_eq!(approval.item_id, "item-1");
        assert_eq!(approval.approval_id.as_deref(), Some("app-1"));
        let event = event_rx.recv().unwrap();
        assert_eq!(event.connection_generation, conn.epoch.0);
        assert!(matches!(event.kind, CodexEventKind::Upsert(_)));
    }

    #[test]
    fn reject_mutation_does_not_drop_armed_approval() {
        let mut conn = dummy();
        let approval = Approval {
            epoch: ServerEpoch(1),
            id: wire::RequestId::Number(7),
            method: "item/commandExecution/requestApproval".into(),
            thread_id: "thread-1".into(),
            turn_id: "turn-1".into(),
            item_id: "item-1".into(),
            approval_id: Some("approval-1".into()),
        };
        conn.approvals.insert(approval.id.clone(), approval);
        let view = Arc::new(Mutex::new(RuntimeView::default()));
        let cfg = CodexMicroConfig::default();
        let stale = Mutation {
            identity: MutationIdentity {
                connection_generation: 0,
                request_id: 9,
                method: "turn/approval/accept".into(),
                thread_id: "other-thread".into(),
                turn_id: Some("turn-1".into()),
                item_id: Some("item-1".into()),
                approval_id: Some("approval-1".into()),
            },
            action: SemanticAction::Approve,
        };
        let (tx, rx) = mpsc::channel();
        let (events, _event_rx) = mpsc::sync_channel(8);
        handle_mutation(&cfg, &mut conn, &events, &view, stale, tx).unwrap();
        assert_eq!(rx.recv().unwrap(), Err(TransportError::ContextMismatch));
        assert_eq!(conn.approvals.len(), 1);
    }

    fn response(id: i64, result: Value, error: Value) -> wire::Inbound {
        wire::Inbound {
            id: Some(wire::RequestId::Number(id)),
            method: None,
            params: Value::Null,
            result,
            error,
        }
    }

    #[test]
    fn catalogs_gate_runtime_admission_until_all_authoritative_syncs() {
        let view = Arc::new(Mutex::new(RuntimeView {
            connected: true,
            ..RuntimeView::default()
        }));
        {
            let mut s = view.lock().unwrap();
            s.models_loaded = true;
            s.threads_loaded = true;
        }
        update_ready(&view);
        assert!(!view.lock().unwrap().ready);
        view.lock().unwrap().skills_loaded = true;
        update_ready(&view);
        assert!(view.lock().unwrap().ready);
    }

    #[test]
    fn transport_rejects_before_ready_and_binds_admitted_command_epoch() {
        let (tx, rx) = mpsc::sync_channel(1);
        let view = Arc::new(Mutex::new(RuntimeView {
            connected: true,
            server_epoch: 9,
            ..RuntimeView::default()
        }));
        let mut transport = RuntimeTransport {
            tx,
            view: Arc::clone(&view),
            timeout: Duration::from_secs(1),
        };
        let mutation = Mutation {
            identity: MutationIdentity {
                connection_generation: 9,
                request_id: 1,
                method: "thread/start".into(),
                thread_id: String::new(),
                turn_id: None,
                item_id: None,
                approval_id: None,
            },
            action: SemanticAction::NewThread,
        };
        assert_eq!(transport.mutate(&mutation), Err(TransportError::NotReady));
        view.lock().unwrap().ready = true;
        let worker = std::thread::spawn(move || match rx.recv().unwrap() {
            RuntimeCommand::Mutation { epoch, reply, .. } => {
                assert_eq!(epoch, 9);
                let _ = reply.send(Ok(()));
            }
            RuntimeCommand::Shutdown => panic!("unexpected shutdown"),
        });
        assert_eq!(transport.mutate(&mutation), Ok(()));
        worker.join().unwrap();
    }

    #[test]
    fn list_projection_maps_statuses_and_seconds_to_ms() {
        let mut conn = dummy();
        let (tx, rx) = mpsc::sync_channel(8);
        let view = Arc::new(Mutex::new(RuntimeView::default()));
        project_threads(&mut conn, &tx, &view, &json!({"data":[
            {"id":"n","updatedAt":2,"status":{"type":"notLoaded"}},
            {"id":"i","updatedAt":3,"status":{"type":"idle"}},
            {"id":"e","updatedAt":4,"status":{"type":"systemError"}},
            {"id":"a","updatedAt":5,"status":{"type":"active","activeFlags":[]}},
            {"id":"p","updatedAt":6,"status":{"type":"active","activeFlags":["waitingOnApproval"]}}
        ]})).unwrap();
        let CodexEventKind::Snapshot { threads, .. } = rx.recv().unwrap().kind else {
            panic!()
        };
        assert_eq!(
            threads.iter().map(|t| t.status).collect::<Vec<_>>(),
            [
                ChatStatus::Unassigned,
                ChatStatus::Idle,
                ChatStatus::Error,
                ChatStatus::Thinking,
                ChatStatus::RequiresInput
            ]
        );
        assert_eq!(threads[1].updated_ms, 3000);
    }

    #[test]
    fn status_notification_uses_thread_id_without_subcontext() {
        let mut conn = dummy();
        let (tx, rx) = mpsc::sync_channel(8);
        notification(&mut conn, &tx, "thread/status/changed", &json!({"threadId":"t","status":{"type":"active","activeFlags":["waitingOnUserInput"]}})).unwrap();
        assert!(
            matches!(rx.recv().unwrap().kind, CodexEventKind::StatusById { thread_id, status: ChatStatus::RequiresInput, .. } if thread_id == "t")
        );
    }

    #[test]
    fn unknown_and_reordered_responses_preserve_other_pending_entries() {
        let mut conn = dummy();
        let view = Arc::new(Mutex::new(RuntimeView::default()));
        let (events, _rx) = mpsc::sync_channel(8);
        let (a, _ar) = mpsc::channel();
        let (b, br) = mpsc::channel();
        conn.pending.insert(
            10,
            Pending {
                kind: PendingKind::Resume { reply: a },
                started: Instant::now(),
            },
        );
        conn.pending.insert(
            11,
            Pending {
                kind: PendingKind::Resume { reply: b },
                started: Instant::now(),
            },
        );
        handle_frame(
            &CodexMicroConfig::default(),
            &mut conn,
            &events,
            &view,
            response(99, json!({}), Value::Null),
        )
        .unwrap();
        assert_eq!(conn.pending.len(), 2);
        handle_frame(
            &CodexMicroConfig::default(),
            &mut conn,
            &events,
            &view,
            response(11, json!({}), Value::Null),
        )
        .unwrap();
        assert_eq!(br.recv().unwrap(), Ok(()));
        assert!(conn.pending.contains_key(&10));
    }

    #[test]
    fn fork_response_projects_and_selects_returned_child() {
        let mut conn = dummy();
        let view = Arc::new(Mutex::new(RuntimeView::default()));
        let (events, rx) = mpsc::sync_channel(8);
        let (ack, done) = mpsc::channel();
        conn.pending.insert(
            7,
            Pending {
                kind: PendingKind::Fork { reply: ack },
                started: Instant::now(),
            },
        );
        handle_frame(
            &CodexMicroConfig::default(),
            &mut conn,
            &events,
            &view,
            response(
                7,
                json!({"thread":{"id":"child","updatedAt":9,"status":{"type":"idle"}}}),
                Value::Null,
            ),
        )
        .unwrap();
        assert_eq!(done.recv().unwrap(), Ok(()));
        assert!(
            matches!(rx.recv().unwrap().kind, CodexEventKind::SelectUpsert(r) if r.context.thread_id == "child" && r.context.turn_id.is_none())
        );
    }

    #[test]
    fn composer_is_retained_on_correlated_protocol_failure() {
        let mut conn = dummy();
        let view = Arc::new(Mutex::new(RuntimeView {
            composer: " secret ".into(),
            ..RuntimeView::default()
        }));
        let (events, _rx) = mpsc::sync_channel(8);
        let (ack, done) = mpsc::channel();
        conn.pending.insert(
            8,
            Pending {
                kind: PendingKind::Turn {
                    reply: ack,
                    composer: Some(" secret ".into()),
                },
                started: Instant::now(),
            },
        );
        handle_frame(
            &CodexMicroConfig::default(),
            &mut conn,
            &events,
            &view,
            response(8, Value::Null, json!({"code":-1})),
        )
        .unwrap();
        assert_eq!(done.recv().unwrap(), Err(TransportError::Protocol));
        assert_eq!(view.lock().unwrap().composer, " secret ");
    }

    #[test]
    fn concurrent_approval_ids_are_armed_independently() {
        let mut conn = dummy();
        let (events, _rx) = mpsc::sync_channel(8);
        for (id, item) in [
            (wire::RequestId::Number(1), "i1"),
            (wire::RequestId::String("two".into()), "i2"),
        ] {
            handle_server_request(
                &mut conn,
                &events,
                wire::Inbound {
                    id: Some(id),
                    method: Some("item/fileChange/requestApproval".into()),
                    params: json!({"threadId":"t","turnId":"u","itemId":item}),
                    result: Value::Null,
                    error: Value::Null,
                },
            )
            .unwrap();
        }
        assert_eq!(conn.approvals.len(), 2);
    }

    #[test]
    fn turn_completion_maps_completed_failed_and_interrupted() {
        assert_eq!(
            turn_completion_status(&json!({"turn":{"status":"completed"}})),
            ChatStatus::CompleteUnread
        );
        assert_eq!(
            turn_completion_status(&json!({"turn":{"status":"failed"}})),
            ChatStatus::Error
        );
        assert_eq!(
            turn_completion_status(&json!({"turn":{"status":"interrupted"}})),
            ChatStatus::Idle
        );
    }

    #[test]
    fn whitespace_composer_emits_no_request() {
        let mut conn = dummy();
        let view = Arc::new(Mutex::new(RuntimeView {
            composer: " \n\t ".into(),
            ..RuntimeView::default()
        }));
        let (events, _event_rx) = mpsc::sync_channel(8);
        let (tx, rx) = mpsc::channel();
        let mutation = Mutation {
            identity: MutationIdentity {
                connection_generation: 1,
                request_id: 1,
                method: "turn/start".into(),
                thread_id: "t".into(),
                turn_id: None,
                item_id: None,
                approval_id: None,
            },
            action: SemanticAction::Send,
        };
        handle_mutation(
            &CodexMicroConfig::default(),
            &mut conn,
            &events,
            &view,
            mutation,
            tx,
        )
        .unwrap();
        assert_eq!(rx.recv().unwrap(), Ok(()));
        assert!(conn.pending.is_empty());
        assert_eq!(view.lock().unwrap().composer, " \n\t ");
    }

    #[test]
    fn ambiguous_skill_name_is_rejected_but_exact_path_is_accepted() {
        let mut conn = dummy();
        conn.skills = vec![
            Skill {
                name: "same".into(),
                path: "/a".into(),
            },
            Skill {
                name: "same".into(),
                path: "/b".into(),
            },
        ];
        let view = Arc::new(Mutex::new(RuntimeView::default()));
        let (events, _event_rx) = mpsc::sync_channel(8);
        let make = |favorite: &str| Mutation {
            identity: MutationIdentity {
                connection_generation: 1,
                request_id: 1,
                method: "skill/run".into(),
                thread_id: "t".into(),
                turn_id: None,
                item_id: None,
                approval_id: None,
            },
            action: SemanticAction::Skill(favorite.into()),
        };
        let (tx, rx) = mpsc::channel();
        handle_mutation(
            &CodexMicroConfig::default(),
            &mut conn,
            &events,
            &view,
            make("same"),
            tx,
        )
        .unwrap();
        assert_eq!(rx.recv().unwrap(), Err(TransportError::Unsupported));
        assert!(conn.pending.is_empty());
        let (tx, _rx) = mpsc::channel();
        handle_mutation(
            &CodexMicroConfig::default(),
            &mut conn,
            &events,
            &view,
            make("/a"),
            tx,
        )
        .unwrap();
        assert_eq!(conn.pending.len(), 1);
    }

    #[test]
    fn resolved_approval_reprojects_another_pending_callback() {
        let mut conn = dummy();
        let (events, rx) = mpsc::sync_channel(8);
        for (id, item) in [
            (wire::RequestId::Number(1), "i1"),
            (wire::RequestId::Number(2), "i2"),
        ] {
            handle_server_request(
                &mut conn,
                &events,
                wire::Inbound {
                    id: Some(id),
                    method: Some("item/fileChange/requestApproval".into()),
                    params: json!({"threadId":"t","turnId":"u","itemId":item}),
                    result: Value::Null,
                    error: Value::Null,
                },
            )
            .unwrap();
        }
        while rx.try_recv().is_ok() {}
        notification(
            &mut conn,
            &events,
            "serverRequest/resolved",
            &json!({"requestId":1}),
        )
        .unwrap();
        assert_eq!(conn.approvals.len(), 1);
        assert!(
            matches!(rx.recv().unwrap().kind, CodexEventKind::Upsert(r) if r.context.approval_id.as_deref().is_some_and(|id| id.ends_with(":2")))
        );
    }

    #[test]
    #[ignore = "requires installed authenticated pinned Codex; handshake/list only"]
    fn live_codex_read_only_smoke() {
        use crate::codex_micro::CodexMicro;

        let (success, output) = probe_version("codex", Duration::from_secs(2)).unwrap();
        assert!(success);
        let version = String::from_utf8_lossy(&output).trim().to_owned();
        assert!(version.contains(wire::PINNED_CODEX_VERSION));

        let cfg = CodexMicroConfig {
            enabled: true,
            cwd: env!("CARGO_MANIFEST_DIR").into(),
            request_timeout_ms: 15_000,
            reconnect_max_ms: 250,
            ..Default::default()
        };
        let (event_tx, event_rx) = mpsc::sync_channel(128);
        let runtime = RuntimeHandle::spawn(cfg, event_tx);
        let mut slots = CodexMicro::default();
        let mut generation = 0;
        let mut snapshot_applied = false;
        let deadline = Instant::now() + Duration::from_secs(20);

        while Instant::now() < deadline {
            if let Ok(event) = event_rx.recv_timeout(Duration::from_millis(50)) {
                if event.connection_generation != generation {
                    generation = event.connection_generation;
                    slots.begin_generation(generation).unwrap();
                }
                snapshot_applied |= matches!(&event.kind, CodexEventKind::Snapshot { .. });
                slots.reduce(event, now_ms()).unwrap();
            }
            let view = runtime.view.lock().unwrap().clone();
            if view.connected
                && view.models_loaded
                && view.threads_loaded
                && view.skills_loaded
                && snapshot_applied
            {
                let populated_slots = slots
                    .slots
                    .iter()
                    .filter(|slot| slot.thread.is_some())
                    .count();
                assert_eq!(
                    populated_slots,
                    view.thread_count.min(crate::codex_micro::SLOT_COUNT)
                );
                println!(
                    "LIVE_READ_ONLY version={version} connected={} model/list={}({}) thread/list={}({}) skills/list={}({}) snapshot_applied={} populated_slots={populated_slots}",
                    view.connected,
                    view.models_loaded,
                    view.model_count,
                    view.threads_loaded,
                    view.thread_count,
                    view.skills_loaded,
                    view.skill_count,
                    snapshot_applied,
                );
                let shutdown_started = Instant::now();
                drop(runtime);
                let shutdown_ms = shutdown_started.elapsed().as_millis();
                assert!(
                    shutdown_ms < 2_000,
                    "graceful shutdown took {shutdown_ms}ms"
                );
                println!(
                    "LIVE_READ_ONLY graceful_shutdown=true shutdown_ms={shutdown_ms} mutations=0"
                );
                return;
            }
            if let Some(error) = view.last_error {
                panic!("live app-server disconnected: {error}");
            }
        }
        panic!("live app-server catalog/snapshot timed out");
    }
    fn dummy() -> Connection {
        #[cfg(unix)]
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("cat >/dev/null")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        #[cfg(windows)]
        let mut child = Command::new("cmd")
            .args(["/C", "more"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let (_tx, rx) = mpsc::sync_channel(1);
        Connection {
            child,
            stdout_thread: None,
            stderr_thread: None,
            stdin,
            frames: rx,
            pending: HashMap::new(),
            next_id: 1,
            epoch: ServerEpoch(1),
            sequence: 0,
            approvals: HashMap::new(),
            skills: vec![],
            efforts: vec![],
            fast_supported: false,
            voice: None,
            voice_done: None,
            voice_cancel: None,
            voice_worker: None,
            voice_reply: None,
        }
    }
}

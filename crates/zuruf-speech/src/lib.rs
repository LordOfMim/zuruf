//! Sample-clock segmentation with immutable, epoch-bound asynchronous results.
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Child,
    sync::{Semaphore, watch},
};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("Audio: {0}")]
    Audio(#[from] hound::Error),
    #[error("Speech backend: {0}")]
    Backend(String),
    #[error("A foreign recording is active")]
    BusyExternal,
    #[error("Not recording")]
    NotRecording,
}
pub type Result<T> = std::result::Result<T, Error>;
pub type AsyncResult<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechState {
    pub state: String,
    pub pending_segments: usize,
    pub last_gap_ms: Option<u64>,
    pub error: Option<String>,
}
#[derive(Debug, Clone)]
pub struct Segment {
    pub id: String,
    pub epoch: String,
    pub annotation_id: Option<String>,
    pub start_sample: u64,
    pub end_sample: u64,
    pub state: SegmentState,
    pub original_text: String,
    pub error: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentState {
    Transcribing,
    Ready,
    Empty,
    Failed,
    Cancelled,
}
pub fn parse_transcription(stdout: &str) -> Result<String> {
    let (_, text) = stdout
        .split_once("\n\n")
        .ok_or_else(|| Error::Backend("Unexpected voxtype transcription output".into()))?;
    Ok(text.trim().to_owned())
}

pub struct SegmentHandle {
    pub id: String,
    receiver: watch::Receiver<Segment>,
}
impl SegmentHandle {
    /// Immutable ownership and sample bounds, available before transcription finishes.
    pub fn segment(&self) -> Segment {
        self.receiver.borrow().clone()
    }
    pub async fn resolve(mut self) -> Segment {
        loop {
            let s = self.receiver.borrow().clone();
            if s.state != SegmentState::Transcribing {
                return s;
            }
            if self.receiver.changed().await.is_err() {
                let mut s = self.receiver.borrow().clone();
                s.state = SegmentState::Failed;
                s.error = Some("worker disappeared".into());
                return s;
            }
        }
    }
}
pub trait Transcriber: Send + Sync + 'static {
    fn transcribe(&self, samples: Vec<i16>) -> AsyncResult<'_, String>;
}
pub trait SpeechAdapter: Send {
    fn state(&self) -> SpeechState;
    fn start(&mut self, epoch: String) -> AsyncResult<'_, ()>;
    /// Seals prior audio and switches the destination atomically before returning.
    fn mark(
        &mut self,
        next_annotation: Option<String>,
        next_epoch: Option<String>,
    ) -> AsyncResult<'_, SegmentHandle>;
    /// Stop capture without invalidating already sealed transcript handles.
    fn finish(&mut self) -> AsyncResult<'_, ()>;
    fn cancel(&mut self) -> AsyncResult<'_, ()>;
}
#[derive(Clone)]
pub struct Voxtype {
    pub executable: PathBuf,
    pub model: Option<String>,
    pub language: Option<String>,
    pub engine: Option<String>,
    pub directory: PathBuf,
    pub keep_audio: bool,
    /// Extra environment for the child only, see [`voxtype_service_env`]. Never logged.
    pub env: Vec<(String, String)>,
}
/// Variables the user's `voxtype.service` gets from its environment files (e.g. the API key
/// of a remote endpoint) and this process lacks. Only `VOXTYPE_*` names are returned, so a
/// per-segment `voxtype transcribe` behaves like the user's own Voxtype. Values must not be logged.
pub async fn voxtype_service_env() -> Vec<(String, String)> {
    let Ok(out) = command("systemctl")
        .args([
            "--user",
            "show",
            "voxtype",
            "-p",
            "EnvironmentFiles",
            "--value",
        ])
        .output()
        .await
    else {
        return vec![];
    };
    let mut env = vec![];
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let path = line.split(" (").next().unwrap_or(line).trim();
        if path.is_empty() {
            continue;
        }
        let Ok(text) = tokio::fs::read_to_string(path).await else {
            continue;
        };
        env.extend(parse_env_file(&text));
    }
    env.retain(|(name, _)| std::env::var_os(name).is_none());
    env
}
/// True when no 100 ms window of 16 kHz audio rises above the noise floor.
fn silent(samples: &[i16]) -> bool {
    const FLOOR: f64 = 200.0;
    !samples.chunks(1600).any(|window| {
        let power = window.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>();
        (power / window.len() as f64).sqrt() > FLOOR
    })
}
fn parse_env_file(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (name, value) = line.split_once('=')?;
            let name = name.trim();
            if !name.starts_with("VOXTYPE_") {
                return None;
            }
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            Some((name.to_owned(), value.to_owned()))
        })
        .collect()
}
impl Transcriber for Voxtype {
    fn transcribe(&self, samples: Vec<i16>) -> AsyncResult<'_, String> {
        Box::pin(async move {
            // Whisper invents sentences for silence; nothing audible means nothing said.
            if silent(&samples) {
                return Ok(String::new());
            }
            zuruf_platform::private_dir(&self.directory)?;
            let path = self
                .directory
                .join(format!("segment-{}.wav", uuid::Uuid::new_v4()));
            let output_path = path.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let mut w = hound::WavWriter::create(
                    output_path,
                    hound::WavSpec {
                        channels: 1,
                        sample_rate: 16000,
                        bits_per_sample: 16,
                        sample_format: hound::SampleFormat::Int,
                    },
                )?;
                for s in samples {
                    w.write_sample(s)?;
                }
                w.finalize()?;
                Ok(())
            })
            .await
            .map_err(|_| Error::Backend("WAV worker failed".into()))??;
            let mut cmd = command(&self.executable);
            cmd.arg("--quiet").envs(self.env.iter().cloned());
            // Without a model Voxtype's own configuration decides (local or remote endpoint);
            // a model named in Zuruf is a local Whisper file and pins local mode.
            if let Some(model) = &self.model {
                cmd.args(["--whisper-mode", "local", "--model", model]);
            }
            for (flag, v) in [("--language", &self.language), ("--engine", &self.engine)] {
                if let Some(v) = v {
                    cmd.args([flag, v]);
                }
            }
            let result = cmd
                .arg("transcribe")
                .arg(&path)
                .kill_on_drop(true)
                .output()
                .await;
            if !self.keep_audio {
                let _ = tokio::fs::remove_file(&path).await;
            }
            let output = result?;
            if !output.status.success() {
                return Err(Error::Backend(format!(
                    "voxtype transcribe exited {:?}",
                    output.status.code()
                )));
            }
            parse_transcription(&String::from_utf8_lossy(&output.stdout))
        })
    }
}
#[derive(Default)]
struct Clock {
    total: u64,
    start: u64,
    samples: Vec<i16>,
    failed: Option<String>,
}
/// PCM sample clock; mark and ingest share a short synchronous lock, never IO.
pub struct Continuous<T: Transcriber> {
    transcriber: Arc<T>,
    clock: Arc<Mutex<Clock>>,
    epoch: String,
    annotation: Option<String>,
    workers: Arc<Semaphore>,
    pending: Arc<std::sync::atomic::AtomicUsize>,
    generation: Arc<std::sync::atomic::AtomicU64>,
    recorder: Option<Child>,
    reader: Option<tokio::task::JoinHandle<()>>,
    active: bool,
    level_tx: watch::Sender<f32>,
}
impl<T: Transcriber> Continuous<T> {
    pub fn new(transcriber: T, workers: usize) -> Self {
        let (level_tx, _) = watch::channel(0.0);
        Self {
            transcriber: Arc::new(transcriber),
            clock: Arc::new(Mutex::new(Clock::default())),
            epoch: String::new(),
            annotation: None,
            workers: Arc::new(Semaphore::new(workers.clamp(1, 2))),
            pending: Arc::default(),
            generation: Arc::default(),
            recorder: None,
            reader: None,
            active: false,
            level_tx,
        }
    }
    pub fn levels(&self) -> watch::Receiver<f32> {
        self.level_tx.subscribe()
    }
    /// Deterministic recorder injection, also used by fake-recorder tests.
    pub fn ingest(&self, samples: &[i16]) -> Result<()> {
        let mut c = self
            .clock
            .lock()
            .map_err(|_| Error::Backend("sample clock poisoned".into()))?;
        c.total += samples.len() as u64;
        c.samples.extend_from_slice(samples);
        Ok(())
    }
    pub fn begin_fake(&mut self, epoch: String) {
        self.epoch = epoch;
        self.active = true;
    }
    pub fn boundary(
        &mut self,
        next_annotation: Option<String>,
        next_epoch: Option<String>,
    ) -> Result<SegmentHandle> {
        if !self.active {
            return Err(Error::NotRecording);
        }
        let (samples, start, end, failure) = {
            let mut c = self
                .clock
                .lock()
                .map_err(|_| Error::Backend("sample clock poisoned".into()))?;
            let s = std::mem::take(&mut c.samples);
            let start = c.start;
            c.start = c.total;
            (s, start, c.total, c.failed.clone())
        };
        let id = uuid::Uuid::new_v4().to_string();
        let segment = Segment {
            id: id.clone(),
            epoch: self.epoch.clone(),
            annotation_id: self.annotation.clone(),
            start_sample: start,
            end_sample: end,
            state: SegmentState::Transcribing,
            original_text: String::new(),
            error: None,
        };
        self.annotation = next_annotation;
        if let Some(e) = next_epoch {
            self.epoch = e;
        }
        let (tx, receiver) = watch::channel(segment.clone());
        let t = self.transcriber.clone();
        let semaphore = self.workers.clone();
        let pending = self.pending.clone();
        let generation = self.generation.clone();
        let current = generation.load(std::sync::atomic::Ordering::SeqCst);
        pending.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        tokio::spawn(async move {
            let mut segment = segment;
            let result = if let Some(f) = failure {
                Err(Error::Backend(f))
            } else if samples.is_empty() {
                Ok(String::new())
            } else {
                match semaphore.acquire_owned().await {
                    Ok(_permit) => t.transcribe(samples).await,
                    Err(_) => Err(Error::Backend("pool closed".into())),
                }
            };
            if generation.load(std::sync::atomic::Ordering::SeqCst) != current {
                segment.state = SegmentState::Cancelled;
            } else {
                match result {
                    Ok(text) => {
                        segment.state = if text.chars().any(char::is_alphanumeric) {
                            SegmentState::Ready
                        } else {
                            SegmentState::Empty
                        };
                        segment.original_text = text;
                    }
                    Err(e) => {
                        segment.state = SegmentState::Failed;
                        segment.error = Some(e.to_string());
                    }
                }
            }
            pending.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            let _ = tx.send(segment);
        });
        Ok(SegmentHandle { id, receiver })
    }
    pub async fn stop_recorder(&mut self) -> Result<()> {
        if let Some(mut child) = self.recorder.take() {
            child.kill().await?;
            child.wait().await?;
        }
        if let Some(reader) = self.reader.take() {
            reader
                .await
                .map_err(|_| Error::Backend("capture task failed".into()))?;
        }
        self.active = false;
        if let Ok(mut clock) = self.clock.lock() {
            clock.failed = None;
        }
        Ok(())
    }
}
impl<T: Transcriber> SpeechAdapter for Continuous<T> {
    fn state(&self) -> SpeechState {
        let failure = self.clock.lock().ok().and_then(|c| c.failed.clone());
        SpeechState {
            state: if failure.is_some() {
                "error"
            } else if self.active {
                "recording"
            } else {
                "idle"
            }
            .into(),
            pending_segments: self.pending.load(std::sync::atomic::Ordering::SeqCst),
            last_gap_ms: None,
            error: failure,
        }
    }
    fn start(&mut self, epoch: String) -> AsyncResult<'_, ()> {
        Box::pin(async move {
            if self.active {
                return Err(Error::Backend("already recording".into()));
            }
            {
                let mut c = self
                    .clock
                    .lock()
                    .map_err(|_| Error::Backend("sample clock poisoned".into()))?;
                c.failed = None;
            }
            let mut child = command("pw-record")
                .args([
                    "--raw",
                    "--rate",
                    "16000",
                    "--channels",
                    "1",
                    "--format",
                    "s16",
                    "-",
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()?;
            let mut stdout = child
                .stdout
                .take()
                .ok_or_else(|| Error::Backend("no audio pipe".into()))?;
            let clock = self.clock.clone();
            let levels = self.level_tx.clone();
            self.reader = Some(tokio::spawn(async move {
                let mut buffer = [0; 4096];
                let mut carry = None;
                let mut last_level = Instant::now();
                loop {
                    match stdout.read(&mut buffer).await {
                        Ok(0) => {
                            if let Ok(mut c) = clock.lock() {
                                c.failed = Some("PipeWire stream ended".into());
                            }
                            break;
                        }
                        Ok(n) => {
                            let mut bytes = Vec::with_capacity(n + 1);
                            if let Some(b) = carry.take() {
                                bytes.push(b);
                            }
                            bytes.extend_from_slice(&buffer[..n]);
                            if bytes.len() % 2 != 0 {
                                carry = bytes.pop();
                            }
                            let samples: Vec<_> = bytes
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                                .collect();
                            if last_level.elapsed() >= Duration::from_millis(66) {
                                let rms = (samples
                                    .iter()
                                    .map(|s| (*s as f32 / 32768.0).powi(2))
                                    .sum::<f32>()
                                    / samples.len().max(1) as f32)
                                    .sqrt();
                                let _ = levels.send(rms);
                                last_level = Instant::now();
                            }
                            if let Ok(mut c) = clock.lock() {
                                c.total += samples.len() as u64;
                                c.samples.extend(samples);
                            }
                        }
                        Err(e) => {
                            if let Ok(mut c) = clock.lock() {
                                c.failed = Some(e.to_string());
                            }
                            break;
                        }
                    }
                }
            }));
            self.recorder = Some(child);
            self.epoch = epoch;
            self.annotation = None;
            self.active = true;
            let startup = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let (samples, failure) = {
                        let c = self
                            .clock
                            .lock()
                            .map_err(|_| Error::Backend("sample clock poisoned".into()))?;
                        (c.samples.len(), c.failed.clone())
                    };
                    if let Some(failure) = failure {
                        return Err(Error::Backend(failure));
                    }
                    if samples > 0 {
                        return Ok(());
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await;
            match startup {
                Ok(Ok(())) => Ok(()),
                other => {
                    let _ = self.stop_recorder().await;
                    match other {
                        Ok(Err(error)) => Err(error),
                        _ => Err(Error::Backend("PipeWire startup timed out".into())),
                    }
                }
            }
        })
    }
    fn mark(
        &mut self,
        annotation: Option<String>,
        epoch: Option<String>,
    ) -> AsyncResult<'_, SegmentHandle> {
        Box::pin(async move { self.boundary(annotation, epoch) })
    }
    fn finish(&mut self) -> AsyncResult<'_, ()> {
        Box::pin(async move { self.stop_recorder().await })
    }
    fn cancel(&mut self) -> AsyncResult<'_, ()> {
        Box::pin(async move {
            self.generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.stop_recorder().await?;
            let mut c = self
                .clock
                .lock()
                .map_err(|_| Error::Backend("sample clock poisoned".into()))?;
            c.samples.clear();
            c.start = c.total;
            Ok(())
        })
    }
}
impl<T: Transcriber> Drop for Continuous<T> {
    fn drop(&mut self) {
        if let Some(child) = &mut self.recorder {
            let _ = child.start_kill();
        }
        if let Some(reader) = self.reader.take() {
            reader.abort();
        }
    }
}

/// Daemon commands are serialized by `&mut self`; only a confirmed owned recording may stop.
struct DaemonIo {
    pub executable: PathBuf,
    pub directory: PathBuf,
    pub model: Option<String>,
    owned: bool,
    epoch: String,
    annotation: Option<String>,
    file: Option<PathBuf>,
    gap: Option<u64>,
    sample: u64,
    started: Instant,
    busy: bool,
}
impl DaemonIo {
    pub fn new(executable: PathBuf, directory: PathBuf) -> Self {
        Self {
            executable,
            directory,
            model: None,
            owned: false,
            epoch: String::new(),
            annotation: None,
            file: None,
            gap: None,
            sample: 0,
            started: Instant::now(),
            busy: false,
        }
    }
    async fn status(&self) -> Result<String> {
        let o = command(&self.executable)
            .args(["status", "--format", "json"])
            .output()
            .await?;
        let v: serde_json::Value = serde_json::from_slice(&o.stdout)
            .map_err(|_| Error::Backend("invalid daemon status".into()))?;
        Ok(v["class"].as_str().unwrap_or("unavailable").into())
    }
    async fn begin(&mut self) -> Result<()> {
        if self.status().await? != "idle" {
            self.busy = true;
            return Err(Error::BusyExternal);
        }
        zuruf_platform::private_dir(&self.directory)?;
        let path = self
            .directory
            .join(format!("daemon-{}.txt", uuid::Uuid::new_v4()));
        let mut follow = command(&self.executable)
            .args(["status", "--follow", "--format", "json"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let stdout = follow
            .stdout
            .take()
            .ok_or_else(|| Error::Backend("status pipe unavailable".into()))?;
        let mut lines = BufReader::new(stdout).lines();
        let mut command = command(&self.executable);
        command
            .args(["record", "start"])
            .arg(format!("--file={}", path.display()))
            .args(["--no-auto-submit", "--no-smart-auto-submit", "--no-osd"]);
        if let Some(model) = &self.model {
            command.args(["--model", model]);
        }
        let out = command.output().await?;
        if !out.status.success() {
            let _ = follow.kill().await;
            return Err(Error::Backend("daemon start rejected".into()));
        }
        let confirmed = tokio::time::timeout(Duration::from_secs(3), async {
            while let Some(line) = lines.next_line().await? {
                if serde_json::from_str::<serde_json::Value>(&line)
                    .ok()
                    .is_some_and(|v| v["class"] == "recording")
                {
                    return Ok::<_, std::io::Error>(true);
                }
            }
            Ok(false)
        })
        .await;
        let _ = follow.kill().await;
        self.owned = true;
        self.file = Some(path);
        self.started = Instant::now();
        self.busy = false;
        if !matches!(confirmed, Ok(Ok(true))) {
            return Err(Error::Backend("record start not confirmed".into()));
        }
        Ok(())
    }
}
impl SpeechAdapter for DaemonIo {
    fn state(&self) -> SpeechState {
        SpeechState {
            state: if self.busy {
                "busy_external"
            } else if self.owned {
                "recording"
            } else {
                "idle"
            }
            .into(),
            pending_segments: 0,
            last_gap_ms: self.gap,
            error: None,
        }
    }
    fn start(&mut self, epoch: String) -> AsyncResult<'_, ()> {
        Box::pin(async move {
            if self.owned {
                return Err(Error::Backend("already recording".into()));
            }
            self.epoch = epoch;
            self.begin().await
        })
    }
    fn mark(
        &mut self,
        annotation: Option<String>,
        epoch: Option<String>,
    ) -> AsyncResult<'_, SegmentHandle> {
        Box::pin(async move {
            if !self.owned {
                return Err(Error::NotRecording);
            }
            // Do not signal a daemon that is no longer recording our segment.
            if self.status().await? != "recording" {
                self.owned = false;
                return Err(Error::BusyExternal);
            }
            let old_epoch = self.epoch.clone();
            let old_annotation = self.annotation.clone();
            self.annotation = annotation;
            if let Some(e) = epoch {
                self.epoch = e;
            }
            let end = self.sample + (self.started.elapsed().as_secs_f64() * 16000.0) as u64;
            let gap = Instant::now();
            let output = command(&self.executable)
                .args(["record", "stop", "--wait", "--json", "--timeout", "120"])
                .kill_on_drop(true)
                .output()
                .await?;
            self.owned = false;
            let parsed = serde_json::from_slice::<serde_json::Value>(&output.stdout);
            let text = parsed
                .as_ref()
                .ok()
                .and_then(|v| v["text"].as_str())
                .unwrap_or("")
                .to_owned();
            let failed = !output.status.success() && output.status.code() != Some(3);
            let segment = Segment {
                id: uuid::Uuid::new_v4().to_string(),
                epoch: old_epoch,
                annotation_id: old_annotation,
                start_sample: self.sample,
                end_sample: end,
                state: if failed {
                    SegmentState::Failed
                } else if text.chars().any(char::is_alphanumeric) {
                    SegmentState::Ready
                } else {
                    SegmentState::Empty
                },
                original_text: text,
                error: if failed {
                    Some("daemon stop failed".into())
                } else {
                    None
                },
            };
            self.sample = end;
            if let Some(file) = self.file.take() {
                let _ = tokio::fs::remove_file(file).await;
            }
            self.begin().await?;
            self.gap = Some(gap.elapsed().as_millis() as u64);
            let id = segment.id.clone();
            let (_tx, receiver) = watch::channel(segment);
            Ok(SegmentHandle { id, receiver })
        })
    }
    fn finish(&mut self) -> AsyncResult<'_, ()> {
        self.cancel()
    }
    fn cancel(&mut self) -> AsyncResult<'_, ()> {
        Box::pin(async move {
            if self.owned {
                if self.status().await? == "recording" {
                    let o = command(&self.executable)
                        .args(["record", "cancel"])
                        .output()
                        .await?;
                    if !o.status.success() {
                        return Err(Error::Backend("cancel failed".into()));
                    }
                }
                self.owned = false;
            }
            if let Some(file) = self.file.take() {
                let _ = tokio::fs::remove_file(file).await;
            }
            Ok(())
        })
    }
}
enum DaemonCommand {
    Start(String, tokio::sync::oneshot::Sender<Result<()>>),
    Mark(Option<String>, Option<String>, watch::Sender<Segment>),
    Cancel(tokio::sync::oneshot::Sender<Result<()>>),
}
/// Serial daemon actor. Boundaries return immediately; stop/finalize/restart runs behind the handle.
pub struct Daemon {
    commands: tokio::sync::mpsc::UnboundedSender<DaemonCommand>,
    status: Arc<Mutex<SpeechState>>,
    pending: Arc<std::sync::atomic::AtomicUsize>,
    epoch: String,
    annotation: Option<String>,
}
impl Daemon {
    pub fn new(executable: PathBuf, directory: PathBuf, model: Option<String>) -> Self {
        let (commands, mut receive) = tokio::sync::mpsc::unbounded_channel();
        let status = Arc::new(Mutex::new(SpeechState {
            state: "idle".into(),
            pending_segments: 0,
            last_gap_ms: None,
            error: None,
        }));
        let shared = status.clone();
        let pending = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let actor_pending = pending.clone();
        tokio::spawn(async move {
            let mut io = DaemonIo::new(executable, directory);
            io.model = model;
            while let Some(command) = receive.recv().await {
                match command {
                    DaemonCommand::Start(epoch, reply) => {
                        let result = io.start(epoch).await;
                        update_daemon_state(&shared, &io, result.as_ref().err());
                        let _ = reply.send(result);
                    }
                    DaemonCommand::Mark(annotation, epoch, reply) => {
                        let mut segment = reply.borrow().clone();
                        match io.mark(annotation, epoch).await {
                            Ok(handle) => {
                                let result = handle.resolve().await;
                                segment.state = result.state;
                                segment.original_text = result.original_text;
                                segment.start_sample = result.start_sample;
                                segment.end_sample = result.end_sample;
                                segment.error = result.error;
                            }
                            Err(error) => {
                                segment.state = SegmentState::Failed;
                                segment.error = Some(error.to_string());
                            }
                        }
                        actor_pending.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                        update_daemon_state(&shared, &io, None);
                        let _ = reply.send(segment);
                    }
                    DaemonCommand::Cancel(reply) => {
                        let result = io.cancel().await;
                        update_daemon_state(&shared, &io, result.as_ref().err());
                        let _ = reply.send(result);
                    }
                }
            }
            let _ = io.cancel().await;
        });
        Self {
            commands,
            status,
            pending,
            epoch: String::new(),
            annotation: None,
        }
    }
}
fn update_daemon_state(shared: &Mutex<SpeechState>, io: &DaemonIo, error: Option<&Error>) {
    if let Ok(mut state) = shared.lock() {
        *state = io.state();
        if let Some(error) = error {
            state.error = Some(error.to_string());
            if !matches!(error, Error::BusyExternal) {
                state.state = "error".into();
            }
        }
    }
}
impl SpeechAdapter for Daemon {
    fn state(&self) -> SpeechState {
        let mut state = self
            .status
            .lock()
            .map(|s| s.clone())
            .unwrap_or(SpeechState {
                state: "error".into(),
                pending_segments: 0,
                last_gap_ms: None,
                error: Some("state lock poisoned".into()),
            });
        state.pending_segments = self.pending.load(std::sync::atomic::Ordering::SeqCst);
        state
    }
    fn start(&mut self, epoch: String) -> AsyncResult<'_, ()> {
        Box::pin(async move {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.commands
                .send(DaemonCommand::Start(epoch.clone(), tx))
                .map_err(|_| Error::Backend("daemon worker stopped".into()))?;
            rx.await
                .map_err(|_| Error::Backend("daemon worker stopped".into()))??;
            self.epoch = epoch;
            Ok(())
        })
    }
    fn mark(
        &mut self,
        annotation: Option<String>,
        epoch: Option<String>,
    ) -> AsyncResult<'_, SegmentHandle> {
        Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let segment = Segment {
                id: id.clone(),
                epoch: self.epoch.clone(),
                annotation_id: self.annotation.clone(),
                start_sample: 0,
                end_sample: 0,
                state: SegmentState::Transcribing,
                original_text: String::new(),
                error: None,
            };
            let (tx, receiver) = watch::channel(segment);
            self.pending
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self
                .commands
                .send(DaemonCommand::Mark(annotation.clone(), epoch.clone(), tx))
                .is_err()
            {
                self.pending
                    .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                return Err(Error::Backend("daemon worker stopped".into()));
            }
            self.annotation = annotation;
            if let Some(epoch) = epoch {
                self.epoch = epoch;
            }

            Ok(SegmentHandle { id, receiver })
        })
    }
    fn finish(&mut self) -> AsyncResult<'_, ()> {
        self.cancel()
    }
    fn cancel(&mut self) -> AsyncResult<'_, ()> {
        Box::pin(async move {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.commands
                .send(DaemonCommand::Cancel(tx))
                .map_err(|_| Error::Backend("daemon worker stopped".into()))?;
            rx.await
                .map_err(|_| Error::Backend("daemon worker stopped".into()))?
        })
    }
}

/// Select continuous, falling back only when capture startup fails; reason is explicit.
pub async fn select(
    voxtype: Voxtype,
    epoch: String,
) -> Result<(Box<dyn SpeechAdapter>, Option<String>)> {
    let mut continuous = Continuous::new(voxtype.clone(), 1);
    match continuous.start(epoch.clone()).await {
        Ok(()) => Ok((Box::new(continuous), None)),
        Err(e) => {
            let mut daemon = Daemon::new(voxtype.executable, voxtype.directory, voxtype.model);
            daemon.start(epoch).await?;
            Ok((Box::new(daemon), Some(e.to_string())))
        }
    }
}

fn command(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    zuruf_platform::background(command.as_std_mut());
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake;
    impl Transcriber for Fake {
        fn transcribe(&self, samples: Vec<i16>) -> AsyncResult<'_, String> {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(if samples[0] == 1 { 40 } else { 1 }))
                    .await;
                if samples[0] < 0 {
                    Err(Error::Backend("fake".into()))
                } else {
                    Ok(if samples[0] == 0 { "..." } else { "recognized" }.into())
                }
            })
        }
    }
    #[test]
    fn voxtype_output() {
        assert_eq!(parse_transcription("Loading audio file: x\nAudio format: 16000 Hz\nProcessing samples...\n\nHallo Welt.\n").unwrap(),"Hallo Welt.");
        assert!(parse_transcription("unexpected output").is_err());
    }
    #[tokio::test]
    async fn epoch_isolation_and_boundaries() {
        let mut c = Continuous::new(Fake, 2);
        c.begin_fake("old".into());
        c.ingest(&[1; 160]).unwrap();
        let old = c
            .boundary(Some("new-annotation".into()), Some("new".into()))
            .unwrap();
        c.ingest(&[2; 80]).unwrap();
        let new = c.boundary(None, None).unwrap().resolve().await;
        let old = old.resolve().await;
        assert_eq!((old.start_sample, old.end_sample), (0, 160));
        assert_eq!((new.start_sample, new.end_sample), (160, 240));
        assert_eq!(old.epoch, "old");
        assert_eq!(new.epoch, "new");
        assert_eq!(old.annotation_id, None);
        assert_eq!(new.annotation_id.as_deref(), Some("new-annotation"));
    }
    #[tokio::test]
    async fn empty_failure_cancel() {
        let mut c = Continuous::new(Fake, 1);
        c.begin_fake("a".into());
        c.ingest(&[0]).unwrap();
        assert_eq!(
            c.boundary(None, None).unwrap().resolve().await.state,
            SegmentState::Empty
        );
        c.ingest(&[-1]).unwrap();
        assert_eq!(
            c.boundary(None, None).unwrap().resolve().await.state,
            SegmentState::Failed
        );
        c.ingest(&[1]).unwrap();
        let pending = c.boundary(None, None).unwrap();
        c.cancel().await.unwrap();
        assert_eq!(pending.resolve().await.state, SegmentState::Cancelled);
        assert!(c.boundary(None, None).is_err());
    }
    struct Counted {
        active: Arc<std::sync::atomic::AtomicUsize>,
        peak: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl Transcriber for Counted {
        fn transcribe(&self, _samples: Vec<i16>) -> AsyncResult<'_, String> {
            Box::pin(async move {
                use std::sync::atomic::Ordering::SeqCst;
                let active = self.active.fetch_add(1, SeqCst) + 1;
                self.peak.fetch_max(active, SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
                self.active.fetch_sub(1, SeqCst);
                Ok("original".into())
            })
        }
    }
    #[tokio::test]
    async fn bounded_workers_and_empty_boundaries() {
        use std::sync::atomic::Ordering::SeqCst;
        let active = Arc::default();
        let peak = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut c = Continuous::new(
            Counted {
                active,
                peak: peak.clone(),
            },
            2,
        );
        c.begin_fake("epoch".into());
        let empty = c.boundary(None, None).unwrap();
        assert_eq!(empty.resolve().await.state, SegmentState::Empty);
        let mut handles = Vec::new();
        for index in 0..12 {
            c.ingest(&[index; 17]).unwrap();
            handles.push(c.boundary(None, None).unwrap());
        }
        assert_eq!(c.state().pending_segments, 12);
        for (index, handle) in handles.into_iter().enumerate() {
            let s = handle.resolve().await;
            assert_eq!(s.start_sample, index as u64 * 17);
            assert_eq!(s.end_sample, (index + 1) as u64 * 17);
            assert_eq!(s.original_text, "original");
        }
        assert_eq!(peak.load(SeqCst), 2);
        assert_eq!(c.state().pending_segments, 0);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn foreign_daemon_is_never_signalled() {
        use std::os::unix::fs::PermissionsExt;
        let directory = std::env::temp_dir().join(format!("zuruf-fake-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir(&directory).await.unwrap();
        let executable = directory.join("voxtype");
        tokio::fs::write(&executable,b"#!/bin/sh\nif [ \"$1\" = status ]; then printf '%s\\n' '{\"class\":\"recording\"}'; else exit 92; fi\n").await.unwrap();
        tokio::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
            .await
            .unwrap();
        let mut daemon = DaemonIo::new(executable, directory.clone());
        assert!(matches!(
            daemon.start("foreign".into()).await,
            Err(Error::BusyExternal)
        ));
        assert_eq!(daemon.state().state, "busy_external");
        daemon.cancel().await.unwrap();
        assert!(!daemon.owned);
        tokio::fs::remove_dir_all(directory).await.unwrap();
    }
    #[tokio::test]
    #[ignore = "records 2 seconds of ambient microphone audio"]
    async fn live_continuous() {
        let v = Voxtype {
            executable: "voxtype".into(),
            model: Some("base.en".into()),
            language: Some("en".into()),
            engine: Some("whisper".into()),
            directory: std::env::temp_dir().join("zuruf-speech-smoke"),
            keep_audio: false,
            env: vec![],
        };
        let mut c = Continuous::new(v, 1);
        c.start("smoke".into()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        let h = c.boundary(None, None).unwrap();
        c.stop_recorder().await.unwrap();
        let segment = h.resolve().await;
        assert!(segment.end_sample > 16000);
        assert!(
            matches!(segment.state, SegmentState::Ready | SegmentState::Empty),
            "{:?}",
            segment
        );
    }
}
#[cfg(test)]
mod env_tests {
    #[test]
    fn silence_is_not_sent_for_transcription() {
        assert!(super::silent(&[]));
        assert!(super::silent(&vec![40; 32000]));
        let mut speech = vec![40; 32000];
        speech[8000..9600].fill(3000);
        assert!(!super::silent(&speech));
    }
    #[test]
    fn only_voxtype_variables_are_taken() {
        let env = super::parse_env_file(
            "# comment\nPATH=/x\nVOXTYPE_WHISPER_API_KEY=\"abc\"\nexport VOXTYPE_OTHER='d e'\nVOXTYPE_RAW=f\n",
        );
        assert_eq!(
            env,
            [
                ("VOXTYPE_WHISPER_API_KEY".to_owned(), "abc".to_owned()),
                ("VOXTYPE_OTHER".to_owned(), "d e".to_owned()),
                ("VOXTYPE_RAW".to_owned(), "f".to_owned()),
            ]
        );
    }
}

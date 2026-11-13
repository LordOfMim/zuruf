//! Synchronous store. Call from a dedicated thread or spawn_blocking.
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use zuruf_core::*;
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("invalid operation: {0}")]
    Invalid(String),
}
pub type Result<T> = std::result::Result<T, StoreError>;
pub fn id() -> String {
    uuid::Uuid::now_v7().to_string()
}
pub fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}
#[derive(Debug, Clone)]
pub struct Paths {
    pub data: PathBuf,
    pub state: PathBuf,
    pub config: PathBuf,
}
impl Paths {
    pub fn from_env() -> Result<Self> {
        #[cfg(windows)]
        let (data, state, config) = {
            let local = std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .ok_or_else(|| StoreError::Invalid("LOCALAPPDATA missing".into()))?
                .join("Zuruf");
            (
                local.join("data"),
                local.join("state"),
                local.join("config"),
            )
        };
        #[cfg(target_os = "macos")]
        let (data, state, config) = {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or_else(|| StoreError::Invalid("HOME missing".into()))?;
            let support = home.join("Library/Application Support/Zuruf");
            (
                support.join("data"),
                support.join("state"),
                support.join("config"),
            )
        };
        #[cfg(not(any(windows, target_os = "macos")))]
        let (data, state, config) = {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or_else(|| StoreError::Invalid("HOME missing".into()))?;
            let xdg = |key, default: &str| {
                std::env::var_os(key)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(default))
            };
            (
                xdg("XDG_DATA_HOME", ".local/share").join("zuruf"),
                xdg("XDG_STATE_HOME", ".local/state").join("zuruf"),
                xdg("XDG_CONFIG_HOME", ".config").join("zuruf"),
            )
        };
        Ok(Self {
            data: std::env::var_os("ZURUF_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or(data),
            state: std::env::var_os("ZURUF_STATE_DIR")
                .map(PathBuf::from)
                .unwrap_or(state),
            config: std::env::var_os("ZURUF_CONFIG_DIR")
                .map(PathBuf::from)
                .unwrap_or(config),
        })
    }
    pub fn prepare(&self) -> Result<()> {
        for p in [&self.data, &self.state, &self.config] {
            private_dir(p)?;
        }
        Ok(())
    }
    pub fn batch_dir(&self, b: &Batch) -> Result<PathBuf> {
        safe_join(
            &self.data,
            &format!(
                "projects/{}/reviews/{}/batches/{}",
                b.project_id, b.review_id, b.id
            ),
        )
    }
}
pub fn private_dir(path: &Path) -> Result<()> {
    zuruf_platform::private_dir(path)?;
    Ok(())
}
pub fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    let p = Path::new(relative);
    if relative.is_empty()
        || relative.contains('\\')
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(StoreError::Invalid("unsafe path".into()));
    }
    let mut current = root.to_path_buf();
    for c in p.components() {
        current.push(c);
        if fs::symlink_metadata(&current).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(StoreError::Invalid("symlink path".into()));
        }
    }
    Ok(current)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    zuruf_platform::atomic_write(path, bytes)?;
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub id: Id,
    pub name: String,
    pub created_at: Timestamp,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedMessage {
    pub task_id: String,
    pub request_id: String,
    pub prompt: String,
}
pub struct Store {
    connection: Connection,
    pub paths: Paths,
}
impl Store {
    pub fn open(paths: Paths) -> Result<Self> {
        paths.prepare()?;
        let db = paths.data.join("zuruf.sqlite");
        let connection = Connection::open(&db)?;
        zuruf_platform::private_file(&db)?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; CREATE TABLE IF NOT EXISTS migrations(version INTEGER PRIMARY KEY);")?;
        if connection.query_row(
            "SELECT count(*) FROM migrations WHERE version=?1",
            [1],
            |row| row.get::<_, i64>(0),
        )? == 0
        {
            connection.execute_batch(include_str!("../migrations/001_initial.sql"))?;
        }
        if connection.query_row(
            "SELECT count(*) FROM migrations WHERE version=?1",
            [2],
            |row| row.get::<_, i64>(0),
        )? == 0
        {
            connection.execute_batch(include_str!("../migrations/002_references.sql"))?;
        }
        if connection.query_row(
            "SELECT count(*) FROM migrations WHERE version=?1",
            [3],
            |row| row.get::<_, i64>(0),
        )? == 0
        {
            connection.execute_batch(include_str!("../migrations/003_messages.sql"))?;
        }
        if connection.query_row(
            "SELECT count(*) FROM migrations WHERE version=4",
            [],
            |row| row.get::<_, i64>(0),
        )? == 0
        {
            connection.execute_batch(include_str!("../migrations/004_entity_timestamps.sql"))?;
        }
        Ok(Self { connection, paths })
    }
    pub fn load(&self) -> Result<Option<Snapshot>> {
        match self
            .connection
            .query_row("SELECT body FROM snapshot WHERE id=1", [], |r| {
                r.get::<_, String>(0)
            }) {
            Ok(s) => Ok(Some(serde_json::from_str(&s)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    pub fn save(&mut self, s: &Snapshot) -> Result<()> {
        self.save_with_events(s, &[])
    }
    pub fn save_with_events(&mut self, s: &Snapshot, events: &[TaskEvent]) -> Result<()> {
        self.save_with_message(s, events, None)
    }
    pub fn save_with_message(
        &mut self,
        s: &Snapshot,
        events: &[TaskEvent],
        message: Option<&QueuedMessage>,
    ) -> Result<()> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO snapshot VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET body=excluded.body",
            [serde_json::to_string(s)?],
        )?;
        tx.execute(
            "DELETE FROM entities WHERE kind != 'participant' AND kind != 'task_event'",
            [],
        )?;
        let value = serde_json::to_value(s)?;
        for (kind, key) in [
            ("project", "projects"),
            ("profile", "profiles"),
            ("session", "sessions"),
            ("review", "reviews"),
            ("batch", "batches"),
            ("annotation", "annotations"),
            ("segment", "segments"),
            ("task", "tasks"),
        ] {
            if let Some(items) = value[key].as_array() {
                for item in items {
                    tx.execute(
                        "INSERT OR REPLACE INTO entities VALUES(?1,?2,?3)",
                        params![kind, item["id"].as_str(), serde_json::to_string(item)?],
                    )?;
                }
            }
        }
        for a in &s.annotations {
            for asset in &a.assets {
                tx.execute(
                    "INSERT OR REPLACE INTO entities VALUES('asset',?1,?2)",
                    params![asset.id, serde_json::to_string(asset)?],
                )?;
            }
        }
        for session in &s.sessions {
            for tab in &session.tabs {
                tx.execute(
                    "INSERT OR REPLACE INTO entities VALUES('tab',?1,?2)",
                    params![tab.id, serde_json::to_string(tab)?],
                )?;
            }
        }
        for (kind, key, relations) in [
            ("session", "sessions", vec![("projectId", "project")]),
            ("review", "reviews", vec![("projectId", "project")]),
            (
                "batch",
                "batches",
                vec![("projectId", "project"), ("reviewId", "review")],
            ),
            (
                "annotation",
                "annotations",
                vec![
                    ("projectId", "project"),
                    ("reviewId", "review"),
                    ("batchId", "batch"),
                ],
            ),
            (
                "segment",
                "segments",
                vec![("batchId", "batch"), ("annotationId", "annotation")],
            ),
            (
                "task",
                "tasks",
                vec![("batchId", "batch"), ("projectId", "project")],
            ),
        ] {
            if let Some(items) = value[key].as_array() {
                for item in items {
                    for (field, parent) in &relations {
                        if let Some(parent_id) = item[field].as_str() {
                            tx.execute(
                                "INSERT INTO entity_links VALUES(?1,?2,?3,?4)",
                                params![kind, item["id"].as_str(), parent, parent_id],
                            )?;
                        }
                    }
                }
            }
        }
        for e in events {
            tx.execute(
                "INSERT INTO entities VALUES('task_event',?1,?2)",
                params![e.id, serde_json::to_string(e)?],
            )?;
        }
        if let Some(m) = message {
            tx.execute(
                "INSERT OR REPLACE INTO messages VALUES(?1,?2)",
                params![m.task_id, serde_json::to_string(m)?],
            )?;
            tx.execute("DELETE FROM receipts WHERE task_id=?1", [&m.task_id])?;
        }
        tx.execute("INSERT INTO entity_timestamps SELECT kind,id,coalesce(json_extract(body,'$.createdAt'),?1),?1 FROM entities WHERE true ON CONFLICT(kind,id) DO UPDATE SET updated_at=excluded.updated_at",[now()])?;
        tx.commit()?;
        Ok(())
    }
    /// Caller mutates a private snapshot; only publish it after this commit succeeds.
    pub fn seal(
        &mut self,
        s: &mut Snapshot,
        batch_id: &str,
        profile_id: Option<&str>,
    ) -> Result<DispatchResponse> {
        self.seal_into(s, batch_id, profile_id, None)
    }
    pub fn seal_into(
        &mut self,
        s: &mut Snapshot,
        batch_id: &str,
        profile_id: Option<&str>,
        follow_up: Option<&str>,
    ) -> Result<DispatchResponse> {
        let result = Self::prepare_seal(s, batch_id, profile_id, follow_up)?;
        self.save(s)?;
        Ok(result)
    }
    pub fn prepare_seal(
        s: &mut Snapshot,
        batch_id: &str,
        profile_id: Option<&str>,
        follow_up: Option<&str>,
    ) -> Result<DispatchResponse> {
        let index = s
            .batches
            .iter()
            .position(|b| b.id == batch_id)
            .ok_or_else(|| StoreError::Invalid("batch missing".into()))?;
        let b = &s.batches[index];
        if b.state != BatchState::Draft {
            return Err(StoreError::Invalid("batch is not draft".into()));
        }
        let count = s.annotations.iter().filter(|a| a.batch_id == b.id).count();
        if count == 0 && b.note.trim().is_empty() {
            return Err(StoreError::Invalid("empty batch".into()));
        }
        let selected = profile_id.or(b.profile_id.as_deref()).or_else(|| {
            s.projects
                .iter()
                .find(|p| p.id == b.project_id)
                .and_then(|p| p.default_profile_id.as_deref())
        });
        let profile = s
            .profiles
            .iter()
            .find(|p| Some(p.id.as_str()) == selected)
            .cloned()
            .ok_or_else(|| StoreError::Invalid("profile required".into()))?;
        let parent = follow_up
            .map(|id| {
                s.tasks
                    .iter()
                    .find(|t| t.id == id && t.project_id == b.project_id && t.thread_id.is_some())
                    .cloned()
                    .ok_or_else(|| StoreError::Invalid("follow-up task missing thread".into()))
            })
            .transpose()?;
        let task = Task {
            id: id(),
            batch_id: b.id.clone(),
            project_id: b.project_id.clone(),
            title: b.title.clone(),
            status: if s.segments.iter().any(|x| {
                x.batch_id == b.id
                    && matches!(
                        x.state,
                        TranscriptSegmentState::Recording | TranscriptSegmentState::Transcribing
                    )
            }) {
                TaskStatus::FinalizingTranscript
            } else {
                TaskStatus::Queued
            },
            remote_status: None,
            profile: parent
                .as_ref()
                .map(|t| t.profile.clone())
                .unwrap_or(profile),
            provider_display_name: None,
            thread_id: parent.as_ref().and_then(|t| t.thread_id.clone()),
            thread_url: None,
            workspace_path: None,
            branch: None,
            last_message: None,
            error: None,
            pending_request: None,
            workspace_conflict: false,
            marked_done: false,
            annotation_count: count as f64,
            created_at: now(),
            dispatched_at: None,
            finished_at: None,
        };
        let draft = Batch {
            id: id(),
            review_id: b.review_id.clone(),
            project_id: b.project_id.clone(),
            seq: b.seq + 1.,
            state: BatchState::Draft,
            title: format!("Aufgabe {}", b.seq + 1.),
            note: String::new(),
            profile_id: b.profile_id.clone(),
            task_id: None,
            created_at: now(),
            sealed_at: None,
        };
        let mut candidate = s.clone();
        candidate.batches[index].state = BatchState::Sealed;
        candidate.batches[index].sealed_at = Some(now());
        candidate.batches[index].task_id = Some(task.id.clone());
        let sealed = candidate.batches[index].clone();
        candidate.batches.push(draft.clone());
        candidate.tasks.push(task.clone());
        candidate.status.active_batch_id = Some(draft.id.clone());
        *s = candidate;
        Ok(DispatchResponse {
            sealed_batch: sealed,
            task,
            active_batch: draft,
        })
    }
    pub fn entity_timestamps(&self, kind: &str, id: &str) -> Result<(String, String)> {
        Ok(self.connection.query_row(
            "SELECT created_at,updated_at FROM entity_timestamps WHERE kind=?1 AND id=?2",
            params![kind, id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }
    pub fn save_participant(&self, participant: &Participant) -> Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO entities VALUES('participant',?1,?2)",
            params![participant.id, serde_json::to_string(participant)?],
        )?;
        Ok(())
    }
    pub fn message(&self, task: &str) -> Result<Option<QueuedMessage>> {
        match self
            .connection
            .query_row("SELECT body FROM messages WHERE task_id=?1", [task], |r| {
                r.get::<_, String>(0)
            }) {
            Ok(s) => Ok(Some(serde_json::from_str(&s)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    pub fn clear_message(&self, task: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM messages WHERE task_id=?1", [task])?;
        Ok(())
    }
    pub fn clear_receipt(&self, task: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM receipts WHERE task_id=?1", [task])?;
        Ok(())
    }
    pub fn receipt(&self, task: &str) -> Result<Option<ThreadReceipt>> {
        match self
            .connection
            .query_row("SELECT body FROM receipts WHERE task_id=?1", [task], |r| {
                r.get::<_, String>(0)
            }) {
            Ok(s) => Ok(Some(serde_json::from_str(&s)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    pub fn save_receipt(&self, task: &str, r: &ThreadReceipt) -> Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO receipts VALUES(?1,?2)",
            params![task, serde_json::to_string(r)?],
        )?;
        Ok(())
    }
    pub fn event(&self, event: &TaskEvent) -> Result<()> {
        self.connection.execute(
            "INSERT INTO entities VALUES('task_event',?1,?2)",
            params![event.id, serde_json::to_string(event)?],
        )?;
        Ok(())
    }
    pub fn events(&self, task: &str) -> Result<Vec<TaskEvent>> {
        let mut q = self
            .connection
            .prepare("SELECT body FROM entities WHERE kind='task_event' ORDER BY id")?;
        let mut out = vec![];
        for row in q.query_map([], |r| r.get::<_, String>(0))? {
            let e: TaskEvent = serde_json::from_str(&row?)?;
            if e.task_id == task {
                out.push(e);
            }
        }
        Ok(out)
    }
    pub fn write_asset(&self, b: &Batch, path: &str, bytes: &[u8]) -> Result<PathBuf> {
        let path = safe_join(&self.paths.batch_dir(b)?, path)?;
        atomic_write(&path, bytes)?;
        Ok(path)
    }
    pub fn settings(&self) -> Result<Settings> {
        let p = self.paths.config.join("settings.json");
        if p.exists() {
            Ok(serde_json::from_slice(&fs::read(p)?)?)
        } else {
            let s = Settings {
                data_dir: self.paths.data.to_string_lossy().into(),
                ..Settings::default()
            };
            self.save_settings(&s)?;
            Ok(s)
        }
    }
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        atomic_write(
            &self.paths.config.join("settings.json"),
            &serde_json::to_vec_pretty(s)?,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assets_private_safe_atomic() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("asset");
        atomic_write(&p, b"a").unwrap();
        atomic_write(&p, b"b").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"b");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o600);
        }
        for s in ["../x", "/x", "x/../../x", "x\\y"] {
            assert!(safe_join(t.path(), s).is_err());
        }
    }
    #[test]
    fn reopen_migration_settings() {
        let t = tempfile::tempdir().unwrap();
        let p = Paths {
            data: t.path().join("d"),
            state: t.path().join("s"),
            config: t.path().join("c"),
        };
        {
            let s = Store::open(p.clone()).unwrap();
            assert!(s.load().unwrap().is_none());
            s.save_settings(&Settings::default()).unwrap();
        }
        let s = Store::open(p).unwrap();
        assert_eq!(s.settings().unwrap(), Settings::default());
    }
}

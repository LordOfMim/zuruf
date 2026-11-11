//! Runtime-only Hyprland bindings. Never opens user configuration.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
#[cfg(unix)]
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::UnixStream,
};
use tokio::{process::Command, sync::watch};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Hyprland: {0}")]
    Compositor(String),
    #[error("Unsupported key: {0}")]
    UnsupportedKey(String),
    #[error("Invalid command or action")]
    InvalidCommand,
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chord {
    pub mods: Vec<String>,
    pub code: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    pub action: String,
    pub scope: String,
    pub chord: Option<Chord>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingIssue {
    pub action: String,
    pub scope: String,
    pub kind: String,
    pub detail: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingBind {
    #[serde(default)]
    pub modmask: u32,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub dispatcher: String,
    #[serde(default)]
    pub arg: String,
    #[serde(default)]
    pub submap: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parser {
    Unavailable,
    Lua,
    Legacy,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeBind {
    pub action: String,
    pub key: String,
    pub mask: u32,
    pub chord: String,
    pub command: String,
    pub description: String,
}
const ACTIONS: &[&str] = &[
    "review.toggle",
    "capture.viewport",
    "capture.region",
    "capture.dom",
    "comment.text",
    "speech.toggle",
    "batch.dispatch",
    "batch.send_to_thread",
    "control.takeover",
    "action.cancel",
    "panel.toggle",
    "app.open",
    "profile.slot1",
    "profile.slot2",
    "profile.slot3",
];
pub fn key_name(code: &str) -> Result<String> {
    if let Some(s) = code.strip_prefix("Key")
        && s.len() == 1
        && s.as_bytes()[0].is_ascii_uppercase()
    {
        return Ok(s.to_lowercase());
    }
    if let Some(s) = code.strip_prefix("Digit")
        && s.len() == 1
        && s.as_bytes()[0].is_ascii_digit()
    {
        return Ok(s.into());
    }
    if let Some(n) = code.strip_prefix('F').and_then(|s| s.parse::<u8>().ok())
        && (1..=24).contains(&n)
    {
        return Ok(format!("F{n}"));
    }
    let key = match code {
        "Space" => "space",
        "Enter" => "Return",
        "Escape" => "Escape",
        "Tab" => "Tab",
        "Backspace" => "BackSpace",
        "Delete" => "Delete",
        "Insert" => "Insert",
        "Home" => "Home",
        "End" => "End",
        "PageUp" => "Prior",
        "PageDown" => "Next",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "Minus" => "minus",
        "Equal" => "equal",
        "BracketLeft" => "bracketleft",
        "BracketRight" => "bracketright",
        "Backslash" => "backslash",
        "Semicolon" => "semicolon",
        "Quote" => "apostrophe",
        "Backquote" => "grave",
        "Comma" => "comma",
        "Period" => "period",
        "Slash" => "slash",
        _ => return Err(Error::UnsupportedKey(code.into())),
    };
    Ok(key.into())
}
fn mask(mods: &[String]) -> Result<u32> {
    let mut mask = 0;
    for m in mods {
        mask |= match m.as_str() {
            "shift" => 1,
            "ctrl" => 4,
            "alt" => 8,
            "super" => 64,
            _ => return Err(Error::InvalidCommand),
        };
    }
    Ok(mask)
}
fn lua_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for b in s.bytes() {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            32..=126 => out.push(b as char),
            _ => out.push_str(&format!("\\{b:03}")),
        }
    }
    out.push('"');
    out
}
pub fn action_command(binary: &Path, action: &str) -> Result<String> {
    if !binary.is_absolute() || !ACTIONS.contains(&action) || binary.to_str().is_none() {
        return Err(Error::InvalidCommand);
    }
    let path = binary.to_string_lossy();
    if path.contains(['\n', '\r', '\0']) {
        return Err(Error::InvalidCommand);
    }
    Ok(format!(
        "'{}' action '{action}'",
        path.replace('\'', "'\\''")
    ))
}
pub fn prepare(
    binary: &Path,
    bindings: &[Binding],
    existing: &[ExistingBind],
) -> (Vec<RuntimeBind>, Vec<BindingIssue>) {
    let mut desired = Vec::new();
    let mut issues = Vec::new();
    let mut used = BTreeSet::new();
    for b in bindings.iter().filter(|b| b.scope == "global") {
        let Some(c) = &b.chord else {
            continue;
        };
        let result = (|| -> Result<RuntimeBind> {
            let key = key_name(&c.code)?;
            let mask = mask(&c.mods)?;
            let command = action_command(binary, &b.action)?;
            let mut names = Vec::new();
            for (bit, name) in [(64, "SUPER"), (1, "SHIFT"), (4, "CTRL"), (8, "ALT")] {
                if mask & bit != 0 {
                    names.push(name.to_owned());
                }
            }
            names.push(key.clone());
            Ok(RuntimeBind {
                action: b.action.clone(),
                key,
                mask,
                chord: names.join(" + "),
                command: command.clone(),
                description: format!("zuruf:{}:{:016x}", b.action, command_fingerprint(&command)),
            })
        })();
        let issue = match result {
            Err(e) => Some(("unsupported_key", e.to_string())),
            Ok(r) => {
                if r.mask == 0 {
                    Some((
                        "needs_modifier",
                        "Global shortcuts require a modifier".into(),
                    ))
                } else if !used.insert((r.mask, r.key.clone())) {
                    Some(("duplicate", "Duplicate global chord".into()))
                } else if let Some(e) = existing.iter().find(|e| {
                    e.modmask == r.mask && e.key.eq_ignore_ascii_case(&r.key) && !is_owned(e)
                }) {
                    Some((
                        "taken_by_system",
                        format!("{}: {} {}", e.description, e.dispatcher, e.arg),
                    ))
                } else {
                    desired.push(r);
                    None
                }
            }
        };
        if let Some((kind, detail)) = issue {
            issues.push(BindingIssue {
                action: b.action.clone(),
                scope: b.scope.clone(),
                kind: kind.into(),
                detail: Some(detail),
            });
        }
    }
    (desired, issues)
}
// Stable command stamp: Lua bind listings expose a callback ID instead of exec text.
fn command_fingerprint(command: &str) -> u64 {
    command.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ byte as u64).wrapping_mul(0x100000001b3)
    })
}
fn is_owned(e: &ExistingBind) -> bool {
    e.description.strip_prefix("zuruf:").is_some_and(|tag| {
        let mut parts = tag.split(':');
        parts.next().is_some_and(|a| ACTIONS.contains(&a))
            && parts
                .next()
                .is_some_and(|hash| hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            && parts.next().is_none()
    })
}
pub fn diff(
    current: &[ExistingBind],
    wanted: &[RuntimeBind],
) -> (Vec<ExistingBind>, Vec<RuntimeBind>) {
    let matches = |e: &ExistingBind, w: &RuntimeBind| {
        e.modmask == w.mask
            && e.key.eq_ignore_ascii_case(&w.key)
            && e.description == w.description
            && (e.dispatcher == "__lua" || (e.dispatcher == "exec" && e.arg == w.command))
    };
    let remove = current
        .iter()
        .filter(|e| is_owned(e) && !wanted.iter().any(|w| matches(e, w)))
        .cloned()
        .collect();
    let add = wanted
        .iter()
        .filter(|w| !current.iter().any(|e| matches(e, w)))
        .cloned()
        .collect();
    (remove, add)
}
async fn run(args: &[String]) -> Result<String> {
    let o = Command::new("hyprctl")
        .args(args)
        .kill_on_drop(true)
        .output()
        .await?;
    let text = String::from_utf8_lossy(&o.stdout).trim().to_owned();
    if !o.status.success() || text.starts_with("Error") || text.starts_with("error") {
        return Err(Error::Compositor(text));
    }
    Ok(text)
}
async fn binds() -> Result<Vec<ExistingBind>> {
    Ok(serde_json::from_str(
        &run(&["binds".into(), "-j".into()]).await?,
    )?)
}
pub struct Hyprland {
    pub parser: Parser,
    binary: PathBuf,
    wanted: Vec<Binding>,
}
impl Hyprland {
    pub async fn detect(binary: PathBuf) -> Result<Self> {
        let parser = if !cfg!(target_os = "linux")
            || std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none()
        {
            Parser::Unavailable
        } else if run(&["eval".into(), "assert(type(hl.bind)==\"function\")".into()])
            .await
            .is_ok_and(|s| s == "ok")
        {
            Parser::Lua
        } else {
            run(&["version".into(), "-j".into()]).await?;
            Parser::Legacy
        };
        Ok(Self {
            parser,
            binary,
            wanted: Vec::new(),
        })
    }
    pub fn available(&self) -> bool {
        self.parser != Parser::Unavailable
    }
    pub async fn apply(&mut self, bindings: Vec<Binding>) -> Result<Vec<BindingIssue>> {
        if !self.available() {
            self.wanted = bindings;
            return Ok(Vec::new());
        }
        let current = binds().await?;
        let (wanted, issues) = prepare(&self.binary, &bindings, &current);
        if !issues.is_empty() {
            return Ok(issues);
        }
        let (remove, add) = diff(&current, &wanted);
        for b in remove {
            self.remove(&b, &current).await?;
        }
        for b in add {
            match self.parser {
                Parser::Lua => {
                    run(&[
                        "eval".into(),
                        format!(
                            "hl.bind({}, hl.dsp.exec_cmd({}), {{description={}}})",
                            lua_quote(&b.chord),
                            lua_quote(&b.command),
                            lua_quote(&b.description)
                        ),
                    ])
                    .await?;
                }
                Parser::Legacy => {
                    if b.command.contains(',') {
                        return Err(Error::InvalidCommand);
                    }
                    run(&[
                        "keyword".into(),
                        "bindd".into(),
                        format!(
                            "{}, {}, {}, exec, {}",
                            legacy_mods(b.mask),
                            b.key,
                            b.description,
                            b.command
                        ),
                    ])
                    .await?;
                }
                Parser::Unavailable => (),
            }
        }
        self.wanted = bindings;
        Ok(issues)
    }
    async fn remove(&self, b: &ExistingBind, current: &[ExistingBind]) -> Result<()> {
        // unbind removes all matches: refuse when a foreign bind shares the chord.
        if current
            .iter()
            .any(|e| e.modmask == b.modmask && e.key.eq_ignore_ascii_case(&b.key) && !is_owned(e))
        {
            return Err(Error::Compositor(
                "Refusing to remove a shared foreign chord".into(),
            ));
        }
        match self.parser {
            Parser::Lua => {
                let chord = format!("{} + {}", lua_mods(b.modmask), b.key);
                run(&["eval".into(), format!("hl.unbind({})", lua_quote(&chord))]).await?;
            }
            Parser::Legacy => {
                run(&[
                    "keyword".into(),
                    "unbind".into(),
                    format!("{},{}", legacy_mods(b.modmask), b.key),
                ])
                .await?;
            }
            Parser::Unavailable => (),
        };
        Ok(())
    }
    pub async fn shutdown(&mut self) -> Result<()> {
        self.apply(Vec::new()).await?;
        Ok(())
    }
    /// Handles reload events and reconnects after compositor socket replacement.
    pub async fn serve(&mut self, stop: watch::Receiver<bool>) -> Result<()> {
        #[cfg(not(unix))]
        {
            let _ = stop;
            Ok(())
        }
        #[cfg(unix)]
        {
            let mut stop = stop;
            if !self.available() {
                return Ok(());
            }
            loop {
                if *stop.borrow() {
                    return self.shutdown().await;
                }
                let socket = match event_socket() {
                    Ok(socket) => socket,
                    Err(error) => {
                        tracing::warn!(error=%error, "Hyprland event socket unavailable");
                        tokio::select! { _=stop.changed()=>return self.shutdown().await, _=tokio::time::sleep(std::time::Duration::from_secs(1))=>() }
                        continue;
                    }
                };
                let stream = UnixStream::connect(socket).await;
                match stream {
                    Ok(stream) => {
                        let issues = self.apply(self.wanted.clone()).await?;
                        if !issues.is_empty() {
                            tracing::warn!(
                                count = issues.len(),
                                "Shortcuts conflict after socket reconnect"
                            );
                        }
                        let mut lines = BufReader::new(stream).lines();
                        loop {
                            tokio::select! {_=stop.changed()=>return self.shutdown().await,line=lines.next_line()=>{match line?{Some(line) if line.starts_with("configreloaded>>")=>{let issues=self.apply(self.wanted.clone()).await?;if !issues.is_empty(){tracing::warn!(count=issues.len(),"Shortcuts conflict after reload");}},Some(_)=>(),None=>break}}}
                        }
                    }
                    Err(e) => tracing::warn!(error=%e,"Hyprland event connection unavailable"),
                }
                tokio::select! {_=stop.changed()=>return self.shutdown().await,_=tokio::time::sleep(std::time::Duration::from_secs(1))=>()}
            }
        }
    }
    pub async fn find_window(&self, pid: u32) -> Result<Option<String>> {
        if !self.available() {
            return Ok(None);
        }
        let v: serde_json::Value =
            serde_json::from_str(&run(&["clients".into(), "-j".into()]).await?)?;
        Ok(v.as_array()
            .and_then(|a| a.iter().find(|w| w["pid"].as_u64() == Some(pid as u64)))
            .and_then(|w| w["address"].as_str())
            .map(str::to_owned))
    }
    pub async fn focus(&self, address: &str) -> Result<()> {
        validate_address(address)?;
        if self.available() {
            if self.parser == Parser::Lua {
                run(&[
                    "dispatch".into(),
                    format!("hl.dsp.focus({{window=\"address:{address}\"}})"),
                ])
                .await?;
            } else {
                run(&[
                    "dispatch".into(),
                    "focuswindow".into(),
                    format!("address:{address}"),
                ])
                .await?;
            }
        }
        Ok(())
    }
    pub async fn arrange(
        &self,
        address: &str,
        width: u32,
        height: u32,
        x: i32,
        y: i32,
    ) -> Result<()> {
        validate_address(address)?;
        if self.parser != Parser::Lua {
            return Err(Error::Compositor(
                "Window arrangement requires verified Lua dispatchers".into(),
            ));
        }
        for expr in [
            format!("hl.dsp.window.float({{action=\"set\",window=\"address:{address}\"}})"),
            format!(
                "hl.dsp.window.resize({{x={width},y={height},relative=false,window=\"address:{address}\"}})"
            ),
            format!(
                "hl.dsp.window.move({{x={x},y={y},relative=false,window=\"address:{address}\"}})"
            ),
        ] {
            run(&["dispatch".into(), expr]).await?;
        }
        Ok(())
    }
}
fn validate_address(a: &str) -> Result<()> {
    if !a.starts_with("0x") || a.len() < 3 || !a[2..].bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::InvalidCommand);
    }
    Ok(())
}
fn legacy_mods(mask: u32) -> String {
    lua_mods(mask).replace(" + ", " ")
}
fn lua_mods(mask: u32) -> String {
    [(64, "SUPER"), (1, "SHIFT"), (4, "CTRL"), (8, "ALT")]
        .into_iter()
        .filter(|(b, _)| mask & b != 0)
        .map(|(_, s)| s)
        .collect::<Vec<_>>()
        .join(" + ")
}
pub fn event_socket() -> Result<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| Error::Compositor("Missing XDG_RUNTIME_DIR".into()))?;
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| Error::Compositor("Missing Hyprland signature".into()))?;
    if signature.contains('/') || signature == ".." {
        return Err(Error::InvalidCommand);
    }
    let directory = PathBuf::from(runtime).join("hypr");
    let current = directory.join(signature).join(".socket2.sock");
    if current.exists() {
        return Ok(current);
    }
    // A restarted compositor can change its signature while this daemon lives.
    // Only select an unambiguous replacement; never guess between sessions.
    let mut candidates = std::fs::read_dir(&directory)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join(".socket2.sock"))
        .filter(|path| path.exists());
    let candidate = candidates
        .next()
        .ok_or_else(|| Error::Compositor("Hyprland event socket missing".into()))?;
    if candidates.next().is_some() {
        return Err(Error::Compositor("Multiple Hyprland event sockets".into()));
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> Binding {
        Binding {
            action: "app.open".into(),
            scope: "global".into(),
            chord: Some(Chord {
                mods: vec!["super".into(), "shift".into(), "ctrl".into(), "alt".into()],
                code: "F24".into(),
            }),
        }
    }
    #[test]
    fn keys() {
        assert_eq!(key_name("KeyN").unwrap(), "n");
        assert_eq!(key_name("Digit4").unwrap(), "4");
        assert_eq!(key_name("ArrowLeft").unwrap(), "Left");
        assert!(key_name("F25").is_err());
        assert!(key_name("Unknown").is_err());
    }
    #[test]
    fn command_injection() {
        assert_eq!(
            action_command(Path::new("/tmp/a'$(id)"), "app.open").unwrap(),
            "'/tmp/a'\\''$(id)' action 'app.open'"
        );
        assert!(action_command(Path::new("/bin/zuruf"), "app.open;id").is_err());
        assert!(action_command(Path::new("relative"), "app.open").is_err());
    }
    #[test]
    fn conflicts_and_diff() {
        let existing: Vec<ExistingBind> =
            serde_json::from_str(include_str!("../tests/binds.json")).unwrap();
        let (w, i) = prepare(Path::new("/bin/zuruf"), &[binding()], &existing);
        assert_eq!(i[0].kind, "taken_by_system");
        assert!(w.is_empty());
        let (w, i) = prepare(Path::new("/bin/zuruf"), &[binding()], &[]);
        assert!(i.is_empty());
        let (r, a) = diff(&existing, &w);
        assert!(r.is_empty());
        assert_eq!(a.len(), 1);
    }
    #[tokio::test]
    #[ignore = "temporarily registers SUPER+SHIFT+CTRL+ALT+F24"]
    async fn live_bind() {
        let mut h = Hyprland::detect("/usr/bin/true".into()).await.unwrap();
        assert!(h.available());
        let before = binds().await.unwrap();
        assert!(!before.iter().any(|b| b.modmask == 77 && b.key == "F24"));
        let result = h.apply(vec![binding()]).await;
        let listed = binds().await;
        let repeated = h.apply(vec![binding()]).await;
        let second = binds().await;
        let cleanup = h.shutdown().await;
        assert!(cleanup.is_ok());
        assert!(repeated.unwrap().is_empty());
        let first = listed
            .as_ref()
            .unwrap()
            .iter()
            .find(|b| b.description.starts_with("zuruf:app.open:"))
            .unwrap();
        let after = second
            .as_ref()
            .unwrap()
            .iter()
            .find(|b| b.description == first.description)
            .unwrap();
        assert_eq!(
            first.arg, after.arg,
            "idempotent apply must retain the Lua callback ID"
        );
        assert!(result.unwrap().is_empty());
        assert!(
            listed
                .unwrap()
                .iter()
                .any(|b| b.description.starts_with("zuruf:app.open:"))
        );
        assert!(
            !binds()
                .await
                .unwrap()
                .iter()
                .any(|b| b.description.starts_with("zuruf:app.open:"))
        );
    }
}

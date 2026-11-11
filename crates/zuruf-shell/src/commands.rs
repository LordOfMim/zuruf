//! The daemon's side channel: one JSON command per line on stdin, events as lines on stdout.
use crate::{options::Theme, shell};
use cef::*;
use serde::Deserialize;
use std::io::{BufRead, Write};

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case")]
pub enum Command {
    /// Open a page in a new tab and show it.
    Open { url: String },
    /// Bring the window to the front.
    Focus,
    /// Show or hide the app panel; toggles when `visible` is absent.
    Panel { visible: Option<bool> },
    /// Wear another theme.
    Theme { mode: Theme },
    /// Show pages at a fixed width in pixels; without `width`, at the full width.
    PageWidth { width: Option<i32> },
    /// Close the window and end the process.
    Quit,
}

wrap_task! {
    struct Run {
        command: Command,
    }

    impl Task {
        fn execute(&self) {
            match self.command.clone() {
                Command::Open { url } => shell::open_tab(&url),
                Command::Focus => shell::focus_window(),
                Command::Panel { visible } => shell::show_panel(visible),
                Command::Theme { mode } => shell::set_theme(mode),
                Command::PageWidth { width } => shell::set_page_width(width),
                Command::Quit => shell::quit(),
            }
        }
    }
}

/// Tell the daemon something happened in the window.
pub fn emit(event: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", serde_json::json!({ "event": event }));
    let _ = out.flush();
}

/// Read commands until stdin closes. A shell whose daemon is gone has no purpose and quits,
/// unless it was started by hand.
pub fn listen(detached: bool) {
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            match serde_json::from_str::<Command>(&line) {
                Ok(command) => {
                    let mut task = Run::new(command);
                    post_task(ThreadId::UI, Some(&mut task));
                }
                Err(error) => eprintln!("zuruf-shell: unknown command: {error}"),
            }
        }
        if !detached {
            let mut task = Run::new(Command::Quit);
            post_task(ThreadId::UI, Some(&mut task));
        }
    });
}

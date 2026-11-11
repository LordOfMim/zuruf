//! Manual lab: one browser session in review mode, without the daemon.
//!
//! ```sh
//! ZURUF_SHELL=target/debug/zuruf-shell \
//!   cargo run -p zuruf-browser --example session -- http://localhost:5173/
//! ```
//!
//! W captures the viewport at once; D lets you choose an element, S draw a region.
//! `ZURUF_LAB_PANEL=<address>` docks a page as the app panel. `ZURUF_LAB_AGENT=1` shows what an
//! agent at work looks like: its pointer, a progress note and an element it points out.
//! Screenshots are written to the directory printed at start; Ctrl+C ends the session.
use std::time::Duration;
use zuruf_browser::{
    Binding, Browser, BrowserEvent, Chord, SessionConfig, bridge::Message, capture::Ring,
};

fn binding(action: &str, code: &str) -> Binding {
    Binding {
        action: action.into(),
        scope: "page".into(),
        chord: Some(Chord {
            mods: vec![],
            code: code.into(),
        }),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let urls: Vec<String> = std::env::args().skip(1).collect();
    let directory = std::env::temp_dir().join(format!("zuruf-lab-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let mut config = SessionConfig::new(directory.join("session"));
    if !urls.is_empty() {
        config.urls = urls;
    }
    config.panel_url = std::env::var("ZURUF_LAB_PANEL").ok();
    if let Ok(theme) = std::env::var("ZURUF_LAB_THEME") {
        config.theme = theme;
    }
    let browser = Browser::start(config).await?;
    browser
        .set_review_mode(
            true,
            vec![
                binding("capture.viewport", "KeyW"),
                binding("capture.region", "KeyS"),
                binding("capture.dom", "KeyD"),
                binding("action.cancel", "Escape"),
            ],
        )
        .await?;
    println!("screenshots: {}", directory.display());
    if std::env::var_os("ZURUF_LAB_AGENT").is_some() {
        tokio::time::sleep(Duration::from_secs(4)).await;
        browser
            .grant("lab".into(), Duration::from_secs(600))
            .await?;
        browser
            .agent_note("Ich prüfe, ob der Zähler nach dem Klick hochzählt.")
            .await;
        browser
            .agent_point("[data-testid=counter]", "Meinst du diesen Knopf?")
            .await?;
        if let Some(tab) = browser.list_tabs().await?.first() {
            browser
                .page
                .agent_pointer(&tab.id, 330.0, 250.0, true)
                .await;
        }
    }

    let mut events = browser.events();
    let mut count = 0;
    loop {
        let event = tokio::select! {
            event = events.recv() => event,
            _ = tokio::signal::ctrl_c() => break,
        };
        match event {
            Ok(BrowserEvent::Page(envelope)) => match &envelope.message {
                Message::Capture { kind, .. } => {
                    match browser.capture_request(&envelope, Ring::default()).await {
                        Ok(shot) => {
                            count += 1;
                            let path = directory.join(format!("capture-{count}-{kind}.png"));
                            let image = if shot.marked_png.is_empty() {
                                &shot.raw_png
                            } else {
                                &shot.marked_png
                            };
                            std::fs::write(&path, image)?;
                            println!("{kind}: {}", path.display());
                            if let Some(dom) = &shot.dom {
                                println!("  <{}> role={:?} name={:?}", dom.tag, dom.role, dom.name);
                            }
                            browser.page.send(Message::Saved, None, None).await;
                        }
                        Err(error) => println!("{kind}: failed: {error}"),
                    }
                }
                Message::Action { action } => println!("action: {action}"),
                _ => {}
            },
            Ok(BrowserEvent::TabsChanged(tabs)) => {
                for tab in tabs {
                    println!(
                        "tab: {} {:?}{}",
                        tab.url,
                        tab.title,
                        if tab.active { " (visible)" } else { "" }
                    );
                }
            }
            Ok(BrowserEvent::Crashed) => {
                println!("browser closed");
                return Ok(());
            }
            Ok(_) => {}
            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
    browser.stop().await?;
    Ok(())
}

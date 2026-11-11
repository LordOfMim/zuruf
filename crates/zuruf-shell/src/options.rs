//! What the daemon tells the shell on its command line.

/// Which of the app's two themes the window wears.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follow the desktop.
    #[default]
    System,
    Dark,
    Light,
}

impl Theme {
    pub fn parse(name: &str) -> Self {
        match name {
            "dark" => Self::Dark,
            "light" => Self::Light,
            _ => Self::System,
        }
    }
}

/// Switches understood by the shell. Everything else on the command line belongs to Chromium.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Profile directory: cookies, storage and cache of this session.
    pub profile: Option<String>,
    /// The app's own page, docked at the left edge. Without it the window is a plain browser.
    pub panel: Option<String>,
    pub panel_width: i32,
    pub theme: Theme,
    /// Width to show pages at; without it they use all the room there is.
    pub page_width: Option<i32>,
    /// Loopback port of the DevTools endpoint the daemon connects to.
    pub port: i32,
    /// Show only the app, filling the window: no tabs, no toolbar.
    pub app_only: bool,
    /// Pages to open as tabs.
    pub urls: Vec<String>,
    /// Keep running when the daemon's end of stdin closes (manual starts and tests).
    pub detached: bool,
}

impl Options {
    pub fn parse(arguments: impl Iterator<Item = String>) -> Self {
        let mut options = Self {
            panel_width: 460,
            ..Self::default()
        };
        for argument in arguments.skip(1) {
            let Some(switch) = argument.strip_prefix("--zuruf-") else {
                continue;
            };
            let (name, value) = switch.split_once('=').unwrap_or((switch, ""));
            match name {
                "profile" => options.profile = Some(value.into()),
                "panel" => options.panel = Some(value.into()),
                "panel-width" => {
                    options.panel_width = value.parse().unwrap_or(options.panel_width);
                }
                "theme" => options.theme = Theme::parse(value),
                "page-width" => options.page_width = value.parse().ok(),
                "port" => options.port = value.parse().unwrap_or(0),
                "url" => options.urls.push(value.into()),
                "detached" => options.detached = true,
                "app-only" => options.app_only = true,
                _ => {}
            }
        }
        if options.urls.is_empty() && !options.app_only {
            options.urls.push("about:blank".into());
        }
        options
    }
}

/// Turn what was typed into the address field into something loadable.
pub fn address(input: &str) -> String {
    let text = input.trim();
    if text.is_empty() {
        return "about:blank".into();
    }
    let has_scheme = text.contains("://")
        || ["about:", "data:", "file:", "chrome:", "view-source:"]
            .iter()
            .any(|scheme| text.starts_with(scheme));
    if has_scheme {
        return text.into();
    }
    let host = text.split(['/', ':', '?', '#']).next().unwrap_or_default();
    let local = host == "localhost"
        || host.ends_with(".localhost")
        || host.parse::<std::net::Ipv4Addr>().is_ok();
    // Development servers rarely speak TLS; everything else should.
    format!("{}://{text}", if local { "http" } else { "https" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches() {
        let options = Options::parse(
            [
                "zuruf-shell",
                "--zuruf-profile=/tmp/p",
                "--zuruf-port=9222",
                "--zuruf-theme=light",
                "--zuruf-url=http://localhost:5173/",
                "--zuruf-url=about:blank",
                "--no-sandbox",
            ]
            .into_iter()
            .map(String::from),
        );
        assert_eq!(options.profile.as_deref(), Some("/tmp/p"));
        assert_eq!(options.port, 9222);
        assert_eq!(options.theme, Theme::Light);
        assert_eq!(options.urls.len(), 2);
        assert!(!options.detached);
    }

    #[test]
    fn typed_addresses() {
        assert_eq!(address("localhost:5173"), "http://localhost:5173");
        assert_eq!(address("127.0.0.1:8080/x"), "http://127.0.0.1:8080/x");
        assert_eq!(address("example.org"), "https://example.org");
        assert_eq!(address("https://a.b/c"), "https://a.b/c");
        assert_eq!(address(" about:blank "), "about:blank");
        assert_eq!(address(""), "about:blank");
    }
}

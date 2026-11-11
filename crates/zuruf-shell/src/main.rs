//! Zuruf's own browser window: embedded Chromium with the app docked beside the page.
//!
//! The daemon starts one shell per browser session, talks to the pages over the DevTools
//! protocol and steers the window with one JSON command per line on stdin.
mod commands;
mod handlers;
mod options;
mod shell;

use cef::*;
use options::Options;

wrap_app! {
    struct ShellApp {
        options: Options,
    }

    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(ShellProcess::new(self.options.clone()))
        }

        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            // Only the main process chooses; its helpers inherit the choice.
            let main_process = process_type.is_none_or(|kind| kind.to_string().is_empty());
            let Some(command_line) = command_line else {
                return;
            };
            if !main_process {
                return;
            }
            // Chromium picks X11 unless told otherwise, even inside a Wayland session, where
            // that means XWayland or no window at all.
            let platform = CefString::from("ozone-platform");
            let chosen = command_line.has_switch(Some(&platform)) == 1;
            if cfg!(target_os = "linux")
                && !chosen
                && std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty())
            {
                command_line
                    .append_switch_with_value(Some(&platform), Some(&CefString::from("wayland")));
            }
        }
    }
}

wrap_browser_process_handler! {
    struct ShellProcess {
        options: Options,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            shell::create(self.options.clone());
            commands::listen(self.options.detached);
        }
    }
}

fn main() {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = args::Args::new();
    let Some(command_line) = args.as_cmd_line() else {
        eprintln!("zuruf-shell: unreadable command line");
        std::process::exit(2);
    };

    // Chromium starts this same executable again for its renderer, GPU and utility processes.
    let helper = command_line.has_switch(Some(&CefString::from("type"))) == 1;
    let code = execute_process(Some(args.as_main_args()), None, std::ptr::null_mut());
    if helper {
        std::process::exit(code);
    }

    let options = Options::parse(std::env::args());
    let sandbox = command_line.has_switch(Some(&CefString::from("no-sandbox"))) != 1;
    let mut settings = Settings {
        no_sandbox: i32::from(!sandbox),
        remote_debugging_port: options.port,
        // The app's own surface, so its window never flashes in the wrong colour.
        background_color: shell::initial_palette(options.theme).background,
        ..Default::default()
    };
    if let Some(profile) = &options.profile {
        settings.root_cache_path = CefString::from(profile.as_str());
    }
    let mut app = ShellApp::new(options);
    let started = initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if started != 1 {
        eprintln!("zuruf-shell: Chromium did not start");
        std::process::exit(1);
    }
    run_message_loop();
    shutdown();
}

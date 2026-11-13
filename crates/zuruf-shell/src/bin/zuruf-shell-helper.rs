//! Chromium's helper processes on macOS (renderer, GPU, utilities).
//!
//! macOS runs every helper from its own small app bundle inside the main bundle's
//! Frameworks folder, so the helper is a separate executable. Linux and Windows start the
//! shell itself again instead and do not use this program.

#[cfg(target_os = "macos")]
fn main() {
    use cef::{args::Args, *};

    let args = Args::new();
    // The sandbox must be in place before anything else runs in the process.
    let mut sandbox = sandbox::Sandbox::new();
    sandbox.initialize(args.as_main_args());

    let Ok(executable) = std::env::current_exe() else {
        std::process::exit(1);
    };
    let loader = library_loader::LibraryLoader::new(&executable, true);
    if !loader.load() {
        eprintln!("zuruf-shell-helper: Chromium Embedded Framework not found");
        std::process::exit(1);
    }
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let code = execute_process(
        Some(args.as_main_args()),
        None::<&mut App>,
        std::ptr::null_mut(),
    );
    drop(loader);
    drop(sandbox);
    std::process::exit(code);
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("zuruf-shell-helper is only used on macOS.");
    std::process::exit(1);
}

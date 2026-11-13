#[cfg(target_os = "linux")]
fn main() {
    // Linux CEF uses OS namespaces and does not take a Windows sandbox pointer.
    let code = unsafe { zuruf_shell::run(cef::args::Args::new(), std::ptr::null_mut()) };
    std::process::exit(code);
}

#[cfg(target_os = "macos")]
fn main() {
    // macOS loads the Chromium framework at run time from the app bundle:
    // Zuruf.app/Contents/MacOS/zuruf-shell → Contents/Frameworks/Chromium Embedded Framework.framework.
    let Ok(executable) = std::env::current_exe() else {
        std::process::exit(1);
    };
    let framework = executable
        .parent()
        .map(|directory| directory.join("../Frameworks/Chromium Embedded Framework.framework"));
    if !framework.is_some_and(|path| path.exists()) {
        eprintln!("zuruf-shell: Chromium Embedded Framework is missing next to the shell");
        std::process::exit(1);
    }
    let loader = cef::library_loader::LibraryLoader::new(&executable, false);
    if !loader.load() {
        eprintln!("zuruf-shell: Chromium Embedded Framework could not be loaded");
        std::process::exit(1);
    }
    if let Err(problem) = zuruf_shell::mac::prepare() {
        eprintln!("zuruf-shell: {problem}");
        std::process::exit(1);
    }
    let code = unsafe { zuruf_shell::run(cef::args::Args::new(), std::ptr::null_mut()) };
    drop(loader);
    std::process::exit(code);
}

#[cfg(windows)]
fn main() {
    eprintln!("Use the packaged CEF bootstrap executable; see docs/releases.md.");
    std::process::exit(1);
}

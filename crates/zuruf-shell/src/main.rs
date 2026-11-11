#[cfg(not(windows))]
fn main() {
    // Linux CEF uses OS namespaces and does not take a Windows sandbox pointer.
    let code = unsafe { zuruf_shell::run(cef::args::Args::new(), std::ptr::null_mut()) };
    std::process::exit(code);
}

#[cfg(windows)]
fn main() {
    eprintln!("Use the packaged CEF bootstrap executable; see docs/releases.md.");
    std::process::exit(1);
}

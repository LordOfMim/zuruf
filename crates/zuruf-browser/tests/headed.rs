#[test]
#[ignore = "opens one real Wayland Chromium; build extension and lab first"]
fn headed_acceptance() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let status = std::process::Command::new(root.join("target/debug/examples/lab"))
        .arg("--check")
        .current_dir(root)
        .status()
        .expect("run prebuilt lab");
    assert!(status.success());
}

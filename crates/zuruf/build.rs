fn main() {
    let output =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("ui");
    let _ = std::fs::remove_dir_all(&output);
    let _ = std::fs::create_dir_all(&output);
    let source = std::path::Path::new("../../apps/ui/dist");
    fn copy(source: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                copy(&entry.path(), &dest.join(entry.file_name()))?;
            } else {
                std::fs::copy(entry.path(), dest.join(entry.file_name()))?;
            }
        }
        Ok(())
    }
    if source.exists()
        && let Err(e) = copy(source, &output)
    {
        println!("cargo:warning=UI copy: {e}");
    }
    println!("cargo:rerun-if-changed=../../apps/ui/dist");
}

fn main() {
    // The shell is installed next to libcef.so and finds it there without a wrapper script.
    // In the build directory the CEF sys crate puts the library and its resources next to
    // the shell as well, so a freshly built shell runs as it is.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo::rustc-link-arg=-Wl,-rpath,$ORIGIN");
    }
}

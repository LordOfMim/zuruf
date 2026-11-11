fn main() {
    // The shell is installed next to libcef.so and finds it there without a wrapper script.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo::rustc-link-arg=-Wl,-rpath,$ORIGIN");
    }
}

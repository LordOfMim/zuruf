//! Embeds the page agent bundle (`apps/page/dist/agent.js`, built with pnpm).
use std::{env, fs, path::PathBuf};

fn main() {
    let source = PathBuf::from("../../apps/page/dist/agent.js");
    let target = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("agent.js");
    println!("cargo::rerun-if-changed={}", source.display());
    match fs::read(&source) {
        Ok(bundle) => fs::write(&target, bundle).expect("write agent bundle"),
        Err(_) => {
            // Keep the crate compiling without Node; pages then simply have no overlay.
            println!(
                "cargo::warning=apps/page/dist/agent.js is missing; run `pnpm --filter @zuruf/page build`"
            );
            fs::write(
                &target,
                "console.warn('Zuruf: page agent bundle was not built');",
            )
            .expect("write agent stub");
        }
    }
}

// The binary SDK version is the build metadata of the locked cef crate.
import { readFileSync } from "node:fs";
const lock = readFileSync(new URL("../../Cargo.lock", import.meta.url), "utf8");
const version = lock.match(/\[\[package\]\]\s+name = "cef"\s+version = "[^"+]+\+([^"\s]+)"/)?.[1];
if (!version) throw new Error("No pinned CEF SDK version in Cargo.lock");
process.stdout.write(version + "\n");

// Fetches the CEF binary distribution the `cef` crate expects and unpacks it in the layout of
// its build script, using the system certificate store. Needed where the crate's own downloader
// cannot reach the CDN (for example behind a TLS-inspecting proxy); elsewhere the build fetches
// CEF by itself.
//
//   node tools/dev/fetch-cef.mjs <cef-version> <target-dir>     e.g. 156.0.26 /build/cef
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";

const [version, root] = process.argv.slice(2);
if (!version || !root) {
  console.error("usage: fetch-cef.mjs <cef-version> <target-dir>");
  process.exit(2);
}
const cdn = process.env.CEF_DOWNLOAD_URL ?? "https://cef-builds.spotifycdn.com";
const platforms = {
  "linux-x64": ["linux64", "cef_linux_x86_64"],
  "linux-arm64": ["linuxarm64", "cef_linux_aarch64"],
  "win32-x64": ["windows64", "cef_windows_x86_64"],
};
const platform = platforms[`${process.platform}-${process.arch}`];
if (!platform) {
  console.error(`unsupported platform ${process.platform}-${process.arch}`);
  process.exit(2);
}
const [indexKey, directoryName] = platform;
const location = join(root, version);
const target = join(location, directoryName);
if (existsSync(join(target, "archive.json")) && existsSync(join(target, "LICENSE.txt"))) {
  console.log(`CEF ${version} already present in ${target}`);
  process.exit(0);
}

const curl = (args) =>
  execFileSync("curl", ["-fL", "--retry", "3", ...args], {
    stdio: ["ignore", "pipe", "inherit"],
    maxBuffer: 1 << 28,
  });
const index = JSON.parse(curl(["-s", `${cdn}/index.json`]).toString());
const build = index[indexKey].versions.find(
  (candidate) =>
    candidate.cef_version === version ||
    candidate.cef_version.startsWith(`${version}+`),
);
if (!build) {
  console.error(`CEF ${version} is not listed for ${indexKey}`);
  process.exit(1);
}
const file = build.files.find((candidate) => candidate.type === "minimal");
mkdirSync(location, { recursive: true });
const archive = join(location, file.name);
console.log(`downloading ${file.name}`);
// Large downloads are sometimes cut off mid-transfer; continue where the last attempt stopped.
for (let attempt = 1; ; attempt++) {
  try {
    curl(["-sS", "-C", "-", "-o", archive, `${cdn}/${encodeURIComponent(file.name)}`]);
    break;
  } catch (error) {
    if (attempt === 20) throw error;
    console.log(`transfer interrupted, resuming (attempt ${attempt + 1})`);
  }
}
const sha1 = createHash("sha1").update(readFileSync(archive)).digest("hex");
if (sha1 !== file.sha1) {
  console.error(`checksum mismatch: ${sha1} != ${file.sha1}`);
  process.exit(1);
}

console.log("unpacking");
// Git Bash's tar interprets drive letters as remote hosts. Python's extractor
// handles native paths and long CEF filenames consistently on Windows.
if (process.platform === "win32") {
  execFileSync("python", ["-c", "import sys, tarfile; tarfile.open(sys.argv[1], 'r:bz2').extractall(sys.argv[2], filter='data')", archive, location], { stdio: "inherit" });
} else {
  execFileSync("tar", ["-xjf", archive, "-C", location], { stdio: "inherit" });
}
const extracted = join(location, file.name.replace(/\.tar\.bz2$/, ""));
rmSync(target, { recursive: true, force: true });
renameSync(join(extracted, "Release"), target);
cpSync(join(extracted, "Resources"), target, { recursive: true });
for (const entry of ["CMakeLists.txt", "cmake", "include", "libcef_dll", "CREDITS.html", "LICENSE.txt"])
  renameSync(join(extracted, entry), join(target, entry));
writeFileSync(
  join(target, "archive.json"),
  JSON.stringify({ type: file.type, name: file.name, sha1: file.sha1 }, null, 2),
);
rmSync(extracted, { recursive: true, force: true });
rmSync(archive);
console.log(`CEF ${build.cef_version} ready in ${target}`);

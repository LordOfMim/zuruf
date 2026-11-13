"""Fail closed if a tagged release is incomplete or differs from its build manifests."""
import hashlib
from pathlib import Path
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
if sys.argv[1:] != [f"v{version}"]:
    raise SystemExit(f"Tag must match Cargo version v{version}")
artifacts = ROOT / "artifacts"
expected = {
    "Zuruf-Setup-windows-x64.exe",
    "zuruf-windows-x64.zip",
    "zuruf-linux-amd64.deb",
    "zuruf-linux-x64.tar.gz",
    "Zuruf-macos-arm64.dmg",
    "zuruf-macos-arm64.zip",
}
verified = {}
for manifest in artifacts.glob("SHA256SUMS-*.txt"):
    for line in manifest.read_text().splitlines():
        checksum, filename = line.split("  ", 1)
        if filename not in expected or filename in verified:
            raise SystemExit("Unexpected or duplicate artifact in checksum manifest")
        with (artifacts / filename).open("rb") as file:
            actual = hashlib.file_digest(file, "sha256").hexdigest()
        if checksum != actual:
            raise SystemExit(f"Checksum mismatch: {filename}")
        verified[filename] = checksum
if set(verified) != expected:
    raise SystemExit("Missing Windows, Linux or macOS package")
(artifacts / "SHA256SUMS.txt").write_text("".join(f"{verified[name]}  {name}\n" for name in sorted(verified)))
print(f"Verified {len(verified)} release artifacts for {version}")

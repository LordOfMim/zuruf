"""Fetch the pinned license-report tool, verifying its independently pinned checksum."""
import hashlib
import os
from pathlib import Path
import platform
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[2]
VERSION = "0.9.3"
TARGETS = {
    "Linux": ("x86_64-unknown-linux-musl", "3ad1e94ea1e63d69d1961f4fb15ccbbe9575d906111f96d2cb4dce6df48f71b4"),
    "Windows": ("x86_64-pc-windows-msvc", "9e2942533ef1c592c4a70bdb384cae1550a3f5fc9ae40379b6662e3d2f16fe02"),
    "Darwin": ("aarch64-apple-darwin", "45520df8680978a6e7c4023ace0e3f311c73efc208cd68e5a7f4ed4822fa0349"),
}
MACHINES = {"Linux": {"x86_64", "amd64"}, "Windows": {"x86_64", "amd64"}, "Darwin": {"arm64"}}


def install():
    if platform.machine().lower() not in MACHINES.get(platform.system(), set()):
        raise SystemExit("Release tooling supports x64 Linux/Windows and Apple silicon macOS hosts")
    target, checksum = TARGETS[platform.system()]
    directory = ROOT / ".release-tools" / f"cargo-about-{VERSION}"
    executable = directory / ("cargo-about.exe" if os.name == "nt" else "cargo-about")
    if executable.exists():
        return executable
    directory.mkdir(parents=True, exist_ok=True)
    archive = directory / "tool.tar.gz"
    url = f"https://github.com/EmbarkStudios/cargo-about/releases/download/{VERSION}/cargo-about-{VERSION}-{target}.tar.gz"
    subprocess.run(["curl", "-fL", "--retry", "3", "-o", str(archive), url], check=True)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != checksum:
        raise SystemExit("cargo-about archive checksum mismatch")
    with tarfile.open(archive) as bundle:
        members = [member for member in bundle.getmembers() if member.isfile() and Path(member.name).name == executable.name]
        if len(members) != 1:
            raise SystemExit("Unexpected cargo-about archive layout")
        executable.write_bytes(bundle.extractfile(members[0]).read())
    executable.chmod(0o755)
    archive.unlink()
    return executable


if __name__ == "__main__":
    print(install())

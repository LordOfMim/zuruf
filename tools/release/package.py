"""Create self-contained runtime bundles and native installers. Build the workspace first."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]


def copy(source, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)


def sha256(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def cef_directory():
    name = "cef_windows_x86_64" if os.name == "nt" else "cef_linux_x86_64"
    root = Path(os.environ["CEF_PATH"]).resolve()
    version = subprocess.check_output(["node", "tools/release/cef-version.mjs"], cwd=ROOT, text=True).strip()
    candidates = [root / version / name, root / name, root]
    return next((candidate for candidate in candidates if (candidate / "archive.json").exists()), None)


def licenses(stage, cef):
    destination = stage / "share/doc/zuruf"
    destination.mkdir(parents=True, exist_ok=True)
    for filename in ["LICENSE", "README.md", "SECURITY.md"]:
        copy(ROOT / filename, destination / filename)
    copy(cef / "LICENSE.txt", destination / "CEF-LICENSE.txt")
    copy(cef / "CREDITS.html", destination / "CHROMIUM-CREDITS.html")
    for package, filename in [("solid-js", "SOLID-LICENSE.txt"), ("@fontsource-variable/inter", "INTER-LICENSE.txt"), ("@fontsource-variable/jetbrains-mono", "JETBRAINS-MONO-LICENSE.txt")]:
        copy(ROOT / "apps/ui/node_modules" / package / "LICENSE", destination / filename)
    tool = subprocess.check_output([sys.executable, "tools/release/install-tools.py"], cwd=ROOT, text=True).strip().splitlines()[-1]
    subprocess.run([tool, "generate", "--workspace", "--fail", "--locked", "--offline", "-o", str(destination / "RUST-LICENSES.html"), "packaging/licenses.hbs"], cwd=ROOT, check=True)


def windows(stage, cef, target, artifacts, version):
    runtime = stage / "lib/zuruf"
    # CEF's Chromium-built host supplies the sandbox broker and calls RunWinMain in our DLL.
    copy(cef / "bootstrap.exe", runtime / "zuruf-shell.exe")
    copy(target / "zuruf_shell.dll", runtime / "zuruf-shell.dll")
    copy(ROOT / "packaging/windows/zuruf-shell.exe.manifest", runtime / "zuruf-shell.exe.manifest")
    copy(target / "zuruf-launcher.exe", stage / "Zuruf.exe")
    installer = artifacts / "Zuruf-Setup-windows-x64.exe"
    compiler = shutil.which("makensis") or str(Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / "NSIS/makensis.exe")
    subprocess.run([compiler, "/INPUTCHARSET", "UTF8", f"/DVERSION={version}", f"/DSTAGE={stage}", f"/DOUTPUT={installer}", "packaging/windows/zuruf.nsi"], cwd=ROOT, check=True)
    with zipfile.ZipFile(artifacts / "zuruf-windows-x64.zip", "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for file in stage.rglob("*"):
            if file.is_file():
                archive.write(file, Path("Zuruf") / file.relative_to(stage))


def linux(stage, artifacts, version):
    runtime = stage / "lib/zuruf"
    subprocess.run(["strip", "--strip-unneeded", str(runtime / "libcef.so")], check=True)
    for name in ["install.sh", "uninstall.sh"]:
        copy(ROOT / "packaging/linux" / name, stage / name)
        (stage / name).chmod(0o755)
    copy(ROOT / "packaging/linux/zuruf.service", stage / "share/zuruf.service")
    with tarfile.open(artifacts / "zuruf-linux-x64.tar.gz", "w:gz", compresslevel=6) as archive:
        archive.add(stage, arcname="zuruf")

    deb = ROOT / ".release-stage/deb"
    if deb.exists():
        shutil.rmtree(deb)
    shutil.copytree(stage / "bin", deb / "usr/bin")
    shutil.copytree(stage / "lib", deb / "usr/lib")
    shutil.copytree(stage / "share/doc", deb / "usr/share/doc")
    copy(ROOT / "assets/icon/zuruf.svg", deb / "usr/share/icons/hicolor/scalable/apps/zuruf.svg")
    unit = (ROOT / "packaging/linux/zuruf.service").read_text().replace("@BINARY@", "/usr/bin/zuruf")
    unit_path = deb / "usr/lib/systemd/user/zuruf.service"
    unit_path.parent.mkdir(parents=True, exist_ok=True)
    unit_path.write_text(unit)
    desktop = deb / "usr/share/applications/zuruf.desktop"
    desktop.parent.mkdir(parents=True, exist_ok=True)
    desktop.write_text("[Desktop Entry]\nType=Application\nName=Zuruf\nComment=Lokales Browser-Review und Agentenaufgaben\nExec=zuruf open\nIcon=zuruf\nTerminal=false\nCategories=Development;\nStartupWMClass=zuruf\n")
    control = deb / "DEBIAN/control"
    control.parent.mkdir(parents=True, exist_ok=True)
    control.write_text(f"Package: zuruf\nVersion: {version}\nArchitecture: amd64\nMaintainer: Zuruf contributors <noreply@github.com>\nSection: devel\nPriority: optional\nHomepage: https://github.com/LordOfMim/zuruf\nDepends: libc6 (>= 2.35), libgtk-3-0, libnss3, libnspr4, libatk1.0-0, libatk-bridge2.0-0, libcups2, libdrm2, libxkbcommon0, libxcomposite1, libxdamage1, libxfixes3, libxrandr2, libgbm1, libasound2, libx11-xcb1, libxshmfence1, libxss1\nDescription: Lokales Browser-Review mit integriertem Chromium\n Screenshots, Annotationen und Feedback direkt an Coding-Agenten in T3 Code.\n")
    subprocess.run(["dpkg-deb", "--build", "--root-owner-group", "-Zxz", "-z3", str(deb), str(artifacts / "zuruf-linux-amd64.deb")], check=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--stage-only", action="store_true")
    args = parser.parse_args()
    if platform.machine().lower() not in {"amd64", "x86_64"}:
        raise SystemExit("Release packages currently target x64")
    cef = cef_directory()
    if cef is None:
        raise SystemExit("CEF_PATH does not contain the pinned SDK")
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "release"
    stage = ROOT / ".release-stage" / ("windows-x64" if os.name == "nt" else "linux-x64")
    artifacts = ROOT / "artifacts"
    if stage.exists():
        shutil.rmtree(stage)
    artifacts.mkdir(exist_ok=True)
    suffix = ".exe" if os.name == "nt" else ""
    copy(target / f"zuruf{suffix}", stage / f"bin/zuruf{suffix}")
    if os.name == "nt":
        copy(target / "zuruf-launcher.exe", stage / "bin/zuruf-launcher.exe")
    runtime = stage / "lib/zuruf"
    runtime.mkdir(parents=True, exist_ok=True)
    # Include runtime files only: no static link archives, symbols, compilers or sandbox helper.
    for file in cef.iterdir():
        if file.is_file() and (file.suffix.lower() in {".dll", ".pak", ".dat", ".bin", ".json", ".so"} or ".so." in file.name):
            if file.name != "archive.json":
                copy(file, runtime / file.name)
    for file in (cef / "locales").glob("*.pak"):
        if file.name.startswith(("de", "en-US")):
            copy(file, runtime / "locales" / file.name)
    if os.name != "nt":
        copy(target / "zuruf-shell", runtime / "zuruf-shell")
        (stage / "bin/zuruf").chmod(0o755)
        (runtime / "zuruf-shell").chmod(0o755)
    copy(ROOT / "assets/icon/zuruf.svg", stage / "share/icons/zuruf.svg")
    licenses(stage, cef)
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    copy(ROOT / "LICENSE", stage / "LICENSE")
    (stage / "BUILD.json").write_text(json.dumps({"version": version, "commit": commit, "cef": json.loads((cef / "archive.json").read_text()), "platform": platform.system(), "arch": "x64"}, indent=2))
    if os.name == "nt":
        # Also set up the stage for --stage-only native smoke tests.
        copy(cef / "bootstrap.exe", runtime / "zuruf-shell.exe")
        copy(target / "zuruf_shell.dll", runtime / "zuruf-shell.dll")
        copy(ROOT / "packaging/windows/zuruf-shell.exe.manifest", runtime / "zuruf-shell.exe.manifest")
        copy(target / "zuruf-launcher.exe", stage / "Zuruf.exe")
    if args.stage_only:
        print(stage)
        return
    if os.name == "nt":
        windows(stage, cef, target, artifacts, version)
    else:
        linux(stage, artifacts, version)
    outputs = sorted(file for file in artifacts.iterdir() if file.suffix in {".exe", ".zip", ".deb", ".gz"})
    manifest = artifacts / f"SHA256SUMS-{'windows' if os.name == 'nt' else 'linux'}-x64.txt"
    manifest.write_text("".join(f"{sha256(file)}  {file.name}\n" for file in outputs))
    print(f"Created {len(outputs)} packages in {artifacts}")


if __name__ == "__main__":
    main()

"""Install, run and uninstall a release in temporary destinations; keep user data."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import zipfile

ROOT = Path(__file__).resolve().parents[2]


def wait_for(description, probe):
    for _ in range(100):
        if probe():
            return
        time.sleep(0.1)
    raise RuntimeError(f"Timed out: {description}")


def smoke(prefix):
    subprocess.run(["node", "tools/release/smoke.mjs", "--prefix", str(prefix)], cwd=ROOT, check=True)


def nsis(executable, destination_option, destination):
    # NSIS parses the final /D= or _?= tail itself; quoting that tail breaks paths
    # containing spaces. Pass a native command line directly, without a shell.
    command = subprocess.list2cmdline([str(executable), "/S"])
    subprocess.run(f"{command} {destination_option}={destination}", check=True)


def main():
    with tempfile.TemporaryDirectory(prefix="zuruf installer test ") as temporary:
        directory = Path(temporary)
        prefix = directory / "Zuruf Installed"
        prefix.mkdir()
        unrelated = prefix / "keep unrelated file.txt"
        unrelated.write_text("not owned by Zuruf")
        if os.name == "nt":
            installer = ROOT / "artifacts/Zuruf-Setup-windows-x64.exe"
            nsis(installer, "/D", prefix)
            wait_for("Windows installation", lambda: (prefix / "Uninstall.exe").is_file())
            assert (prefix / "Zuruf.exe").is_file()
            smoke(prefix)
            marker = Path(os.environ["LOCALAPPDATA"]) / "Zuruf/installer-preservation-test.txt"
            marker.parent.mkdir(parents=True, exist_ok=True)
            assert not marker.exists()
            try:
                marker.write_text("keep this data")
                nsis(installer, "/D", prefix)
                nsis(prefix / "Uninstall.exe", "_?", prefix)
                wait_for("Windows uninstall", lambda: not (prefix / "bin/zuruf.exe").exists())
                assert marker.read_text() == "keep this data"
            finally:
                marker.unlink(missing_ok=True)
            with zipfile.ZipFile(ROOT / "artifacts/zuruf-windows-x64.zip") as archive:
                assert "Zuruf/Zuruf.exe" in archive.namelist()
                assert "Zuruf/lib/zuruf/zuruf-shell.dll" in archive.namelist()
        else:
            env = {**os.environ, "PREFIX": str(prefix), "XDG_DATA_HOME": str(directory / "data"), "XDG_CONFIG_HOME": str(directory / "config"), "XDG_STATE_HOME": str(directory / "state"), "ZURUF_INSTALL_TEST": "1"}
            bundle = ROOT / ".release-stage/linux-x64"
            subprocess.run(["bash", str(bundle / "install.sh")], cwd=bundle, env=env, check=True)
            smoke(prefix)
            marker = directory / "data/zuruf/installer-preservation-test.txt"
            marker.parent.mkdir(parents=True, exist_ok=True)
            marker.write_text("keep this data")
            subprocess.run(["bash", str(bundle / "install.sh")], cwd=bundle, env=env, check=True)
            subprocess.run(["bash", str(bundle / "uninstall.sh")], env=env, check=True)
            assert not (prefix / "bin/zuruf").exists()
            assert marker.read_text() == "keep this data"
            extracted = directory / "deb"
            subprocess.run(["dpkg-deb", "--extract", str(ROOT / "artifacts/zuruf-linux-amd64.deb"), str(extracted)], check=True)
            smoke(extracted / "usr")
        assert unrelated.read_text() == "not owned by Zuruf"
        print("PASS installer update/uninstall, preserved user data and unrelated files, packaged browser")


if __name__ == "__main__":
    main()

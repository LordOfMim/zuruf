//! Small OS boundaries shared by the daemon and its adapters.
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

/// Use the operating system's cryptographic RNG on every supported platform.
pub fn random_token() -> io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    protect(path, true)
}

pub fn private_file(path: &Path) -> io::Result<()> {
    protect(path, false)
}

#[cfg(unix)]
fn protect(path: &Path, directory: bool) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
    )
}

#[cfg(windows)]
fn protect(path: &Path, directory: bool) -> io::Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SetFileSecurityW,
        },
    };
    // Owner Rights and SYSTEM only. Child files/directories inherit the private ACL.
    let sddl = if directory {
        "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)"
    } else {
        "D:P(A;;FA;;;OW)(A;;FA;;;SY)"
    };
    let sddl: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    // Rust's filesystem supports long paths. Pass its verbatim Windows path to Win32 too,
    // otherwise nested batch/screenshot paths exceed the legacy MAX_PATH limit here.
    let canonical = fs::canonicalize(path)?;
    let path: Vec<u16> = canonical.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut descriptor = ptr::null_mut();
    // SAFETY: Both strings are NUL-terminated; the descriptor is freed with LocalFree.
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let applied = SetFileSecurityW(
            path.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        );
        let error = if applied == 0 {
            Some(io::Error::last_os_error())
        } else {
            None
        };
        LocalFree(descriptor);
        if let Some(error) = error {
            return Err(error);
        }
    }
    Ok(())
}

/// Write a private sibling, flush it, and replace the destination atomically.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("no parent directory"))?;
    private_dir(parent)?;
    let temporary = parent.join(format!(".{}.tmp", random_token()?));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        private_file(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

/// Background processes should never flash a console when launched from the Windows GUI.
pub fn background(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// A process that outlives the one starting it. On Unix it leaves the caller's process group,
/// so neither Ctrl+C in a terminal nor the end of the launching app reaches it.
pub fn detach(command: &mut std::process::Command) {
    background(command);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

/// Whether `shell` can enforce a memory limit here. Only Linux with a systemd user manager can.
pub fn memory_limit_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        let on_path = std::env::var_os("PATH").is_some_and(|paths| {
            std::env::split_paths(&paths).any(|dir| dir.join("systemd-run").is_file())
        });
        on_path && Path::new("/run/systemd/system").exists()
    }
    #[cfg(not(target_os = "linux"))]
    false
}

/// A command line the user wrote, run by the platform's shell as the leader of its own
/// process group, so `stop_tree` reaches everything it starts. With a limit, and where
/// `memory_limit_supported`, the whole tree runs in a transient systemd scope capped at it.
pub fn shell(command_line: &str, memory_limit_mb: Option<u64>) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = memory_limit_mb;
        let mut command = std::process::Command::new("cmd");
        // cmd parses its own line; Rust's argument quoting would change what the user wrote.
        command.arg("/C").raw_arg(command_line);
        background(&mut command);
        command
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let limit = memory_limit_mb.filter(|_| memory_limit_supported());
        let mut command = match limit {
            Some(megabytes) => {
                let mut scoped = std::process::Command::new("systemd-run");
                scoped
                    .args(["--user", "--scope", "--quiet", "--collect", "-p"])
                    .arg(format!("MemoryMax={megabytes}M"))
                    .args(["-p", "MemorySwapMax=0", "--", "sh", "-c", command_line]);
                scoped
            }
            None => {
                let mut plain = std::process::Command::new("sh");
                plain.args(["-c", command_line]);
                plain
            }
        };
        command.process_group(0);
        command
    }
}

/// End a process started by `shell` together with everything it started.
/// Without `force` the processes are asked to stop and may clean up.
pub fn stop_tree(pid: u32, force: bool) -> io::Result<()> {
    #[cfg(windows)]
    {
        let mut command = std::process::Command::new("taskkill");
        command.args(["/PID", &pid.to_string(), "/T"]);
        if force {
            command.arg("/F");
        }
        background(&mut command);
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|_| ())
    }
    #[cfg(unix)]
    {
        // The system call, not the `kill` program: some versions of that program read a
        // negative id as an option and then signal every process of the user.
        let group = libc::pid_t::try_from(pid)
            .ok()
            .filter(|group| *group > 1)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a process group"))?;
        let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
        // SAFETY: kill takes plain integers. A negative id addresses the process group, and
        // the check above rules out 0 and -1, which would reach beyond it.
        let result = unsafe { libc::kill(-group, signal) };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        // The group is already gone: nothing left to stop.
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(());
        }
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_files_can_be_replaced_without_leaving_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state/credential.json");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn credentials_have_full_length_and_are_unique() {
        let first = random_token().unwrap();
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, random_token().unwrap());
    }

    #[test]
    fn private_files_support_long_nested_paths() {
        let directory = tempfile::tempdir().unwrap();
        let mut path = directory.path().to_owned();
        for _ in 0..4 {
            path.push("nested-state-".repeat(6));
        }
        path.push("credential.json");
        assert!(path.as_os_str().len() > 260);
        atomic_write(&path, b"private state").unwrap();
        assert_eq!(fs::read(path).unwrap(), b"private state");
    }

    #[cfg(unix)]
    #[test]
    fn a_shell_command_and_its_children_end_together() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("child");
        let line = format!("(sleep 30; touch {}) & wait", marker.display());
        let mut child = shell(&line, None).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        stop_tree(child.id(), true).unwrap();
        let status = child.wait().unwrap();
        assert!(!status.success());
        assert!(!marker.exists());
    }
}

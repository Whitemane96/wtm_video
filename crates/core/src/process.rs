use std::path::Path;
use std::process::Command;

/// Builds a `Command` that behaves the same on every OS.
///
/// On Windows a GUI app spawning a console program flashes a terminal window
/// unless `CREATE_NO_WINDOW` is set. yt-dlp is a Python program, so force UTF-8
/// I/O to avoid mangled titles on Windows' legacy code pages.
pub fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("PYTHONIOENCODING", "utf-8").env("PYTHONUTF8", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group, so `kill_tree` can stop everything it starts.
        cmd.process_group(0);
    }
    cmd
}

/// Kills a child and everything it spawned.
///
/// The standalone yt-dlp (on every OS) is a PyInstaller launcher that starts a
/// second process, so killing only the parent would leave the download running.
pub fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        let _ = command(Path::new("taskkill"))
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .output();
    }
    #[cfg(unix)]
    {
        // A negative pid means "the whole process group", which the child leads
        // (see `command`). Errors are ignored: it may already have exited.
        // SAFETY: a plain syscall taking two integers.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::thread::sleep;
    use std::time::Duration;

    /// `ps` reports nothing (or a zombie) once a process is really gone.
    fn is_running(pid: u32) -> bool {
        let out = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .expect("ps runs");
        let stat = String::from_utf8_lossy(&out.stdout);
        let stat = stat.trim();
        !stat.is_empty() && !stat.starts_with('Z')
    }

    /// Cancel must stop the launcher's child too, not just the launcher.
    #[test]
    fn kill_tree_also_kills_grandchildren() {
        let pidfile = std::env::temp_dir().join(format!("wtm-killtree-{}", std::process::id()));
        let script = format!("sleep 60 & echo $! > '{}'; wait", pidfile.display());
        let mut child = command(Path::new("sh")).args(["-c", &script]).spawn().expect("sh starts");

        let mut grandchild = None;
        for _ in 0..50 {
            if let Ok(text) = fs::read_to_string(&pidfile)
                && let Ok(pid) = text.trim().parse::<u32>()
            {
                grandchild = Some(pid);
                break;
            }
            sleep(Duration::from_millis(100));
        }
        let grandchild = grandchild.expect("shell wrote the grandchild's pid");
        assert!(is_running(grandchild), "grandchild should be running before the kill");

        kill_tree(&mut child);

        for _ in 0..50 {
            if !is_running(grandchild) {
                break;
            }
            sleep(Duration::from_millis(100));
        }
        let survived = is_running(grandchild);
        if survived {
            // Don't leave a stray process behind, even though the test is failing.
            let _ = std::process::Command::new("kill").args(["-9", &grandchild.to_string()]).status();
        }
        let _ = fs::remove_file(&pidfile);
        assert!(!survived, "grandchild {grandchild} survived kill_tree");
    }
}

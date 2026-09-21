use std::io::{BufRead, BufReader, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::download::{CancelToken, DownloadError};
use crate::process::kill_tree;

/// Runs `cmd` to completion, passing each line of its stdout to `on_line` as it
/// arrives. Returns the exit status and everything it wrote to stderr, or
/// `Cancelled` if the token fires first (the whole process tree is killed).
pub(crate) fn run_streaming(
    mut cmd: Command,
    name: &str,
    cancel: &CancelToken,
    mut on_line: impl FnMut(&str),
) -> Result<(ExitStatus, Vec<u8>), DownloadError> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| DownloadError::Failed(format!("could not start {name}: {e}")))?;

    // Drain both pipes on their own threads so neither can fill and stall the child.
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (tx, rx) = mpsc::channel::<String>();
    let out_reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let err_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = BufReader::new(stderr).read_to_end(&mut buf);
        buf
    });

    loop {
        if cancel.is_cancelled() {
            kill_tree(&mut child);
            return Err(DownloadError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => on_line(&line),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let status = child
        .wait()
        .map_err(|e| DownloadError::Failed(e.to_string()))?;
    let _ = out_reader.join();
    let stderr = err_reader.join().unwrap_or_default();
    Ok((status, stderr))
}

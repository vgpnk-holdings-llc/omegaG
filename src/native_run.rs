//! Native subprocess execution with a hard timeout (Linux).
//!
//! Used by `detect` to probe tmux natively (the Windows build probes through
//! WSL instead — see `wsl.rs`). No shell is ever involved: argv is passed
//! directly to `execvp` via [`std::process::Command`].
//!
//! Semantics mirror `wsl::run_wsl` where it matters for `detect`:
//! - stdout is captured and returned on success;
//! - non-zero exit status, spawn failure, or timeout are all errors, so the
//!   caller degrades to the next fallback exactly like the "no WSL" path.
#![cfg(target_os = "linux")]

use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Poll interval while waiting for the child to exit.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Run `argv` (program + args, no shell), capturing stdout.
///
/// Returns:
/// - `Ok(stdout)` when the process exits 0 within `timeout_ms`,
/// - `Err(NotFound)` when the program does not exist,
/// - `Err(TimedOut)` when the deadline passes — the child is killed and
///   reaped before returning,
/// - `Err(Other)` on non-zero exit or I/O failure.
pub fn run(argv: &[&str], timeout_ms: u64) -> io::Result<String> {
    let (prog, args) = argv
        .split_first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "native_run: empty argv"))?;

    let mut command = Command::new(prog);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // Own process group (child is the leader) so a timeout can kill the
        // whole tree — killing only the direct child would leave
        // grandchildren (e.g. a `sleep` inside a wrapper script) holding the
        // stdout pipe open and stalling the reader thread past the deadline.
        .process_group(0);
    let mut child = command.spawn()?;

    // Drain stdout on a helper thread so a child writing more than the pipe
    // buffer cannot deadlock against our try_wait loop.
    let mut stdout_pipe = child.stdout.take().expect("stdout is piped");
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let buf = reader.join().unwrap_or_default();
                return if status.success() {
                    Ok(String::from_utf8_lossy(&buf).into_owned())
                } else {
                    Err(io::Error::other(format!("{prog} exited with {status}")))
                };
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    kill_tree(&mut child); // also EOFs the reader thread
                    let _ = reader.join();
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("{prog} timed out after {timeout_ms} ms"),
                    ));
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => {
                kill_tree(&mut child);
                let _ = reader.join();
                return Err(e);
            }
        }
    }
}

/// Kill the child's whole process group, then reap the child.
fn kill_tree(child: &mut std::process::Child) {
    // Child is its group leader (process_group(0) at spawn).
    unsafe {
        libc::killpg(child.id() as i32, libc::SIGKILL);
    }
    let _ = child.kill(); // belt and braces if the group signal raced a re-parent
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_stdout() {
        let out = run(&["/bin/echo", "hello"], 5_000).expect("echo must succeed");
        assert_eq!(out, "hello\n");
    }

    #[test]
    fn passes_args_without_shell() {
        // `printf %s a b c` would need shell quoting through a shell; direct
        // argv must reproduce it byte-for-byte.
        let out = run(&["/bin/printf", "%s", "a b", "c"], 5_000).expect("printf must succeed");
        assert_eq!(out, "a bc");
    }

    #[test]
    fn missing_program_is_not_found() {
        let err = run(&["/definitely/not/a/real/binary-ds4cc"], 1_000).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn non_zero_exit_is_error() {
        let err = run(&["/bin/sh", "-c", "exit 3"], 5_000).unwrap_err();
        assert!(err.to_string().contains("exit"));
    }

    #[test]
    fn timeout_kills_child() {
        let start = Instant::now();
        let err = run(&["/bin/sleep", "30"], 150).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "kill-on-timeout must not wait for the child to finish"
        );
    }

    #[test]
    fn timeout_kills_whole_process_tree() {
        // Regression: a wrapper whose grandchild outlives the direct child.
        // Without process-group kill, the grandchild keeps the stdout pipe
        // open and the reader thread stalls ~30s past the deadline.
        let start = Instant::now();
        let err = run(&["/bin/sh", "-c", "sleep 30; sleep 30"], 200).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "process-tree kill took {:?}",
            start.elapsed()
        );
    }
}

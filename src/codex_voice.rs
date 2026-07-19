//! Direct-process voice adapter. Capture begins on PTT press; release closes
//! stdin and waits for the already-running adapter's bounded transcript.

use std::io::Read;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum VoiceError {
    Disabled,
    ExecutableNotAbsolute,
    Spawn,
    Timeout,
    OutputTooLarge,
    Failed,
}

pub struct VoiceCapture {
    child: Arc<Mutex<Option<Child>>>,
    stdin: Option<ChildStdin>,
    reader: Option<JoinHandle<(Vec<u8>, bool)>>,
    cancel: Arc<AtomicBool>,
}

impl VoiceCapture {
    pub fn start(argv: &[String], output_limit: usize) -> Result<Self, VoiceError> {
        validate(argv)?;
        let mut command = Command::new(&argv[0]);
        command
            .args(&argv[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|_| VoiceError::Spawn)?;
        let stdin = child.stdin.take().ok_or(VoiceError::Spawn)?;
        let mut stdout = child.stdout.take().ok_or(VoiceError::Spawn)?;
        let reader = std::thread::spawn(move || {
            let mut kept = Vec::new();
            let mut chunk = [0; 4096];
            let mut overflow = false;
            loop {
                match stdout.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) if kept.len().saturating_add(n) <= output_limit => {
                        kept.extend_from_slice(&chunk[..n])
                    }
                    Ok(_) => overflow = true,
                }
            }
            (kept, overflow)
        });
        Ok(Self {
            child: Arc::new(Mutex::new(Some(child))),
            stdin: Some(stdin),
            reader: Some(reader),
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }

    #[cfg(test)]
    pub fn is_running(&self) -> bool {
        self.child
            .lock()
            .expect("voice child poisoned")
            .as_mut()
            .is_some_and(|c| c.try_wait().ok().flatten().is_none())
    }

    pub fn cancel_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    pub fn finish(mut self, timeout_ms: u64, composer_limit: usize) -> Result<String, VoiceError> {
        self.stdin.take(); // EOF is the adapter's stop signal.
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let status = loop {
            let mut guard = self.child.lock().expect("voice child poisoned");
            let child = guard.as_mut().ok_or(VoiceError::Failed)?;
            if self.cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
                let timed_out = !self.cancel.load(Ordering::Acquire);
                terminate(child);
                let _ = child.wait();
                guard.take();
                return Err(if timed_out {
                    VoiceError::Timeout
                } else {
                    VoiceError::Failed
                });
            }
            if let Some(status) = child.try_wait().map_err(|_| VoiceError::Failed)? {
                guard.take();
                break status;
            }
            drop(guard);
            std::thread::sleep(Duration::from_millis(10));
        };
        let (bytes, overflow) = self
            .reader
            .take()
            .ok_or(VoiceError::Failed)?
            .join()
            .map_err(|_| VoiceError::Failed)?;
        if overflow {
            return Err(VoiceError::OutputTooLarge);
        }
        if !status.success() {
            return Err(VoiceError::Failed);
        }
        Ok(String::from_utf8_lossy(&bytes)
            .trim()
            .chars()
            .take(composer_limit)
            .collect())
    }

    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.stdin.take();
        if let Some(mut child) = self.child.lock().expect("voice child poisoned").take() {
            terminate(&mut child);
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for VoiceCapture {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn terminate(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
}

fn validate(argv: &[String]) -> Result<(), VoiceError> {
    let Some(program) = argv.first() else {
        return Err(VoiceError::Disabled);
    };
    if !Path::new(program).is_absolute() {
        return Err(VoiceError::ExecutableNotAbsolute);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_absolute_argv() {
        assert!(matches!(
            VoiceCapture::start(&["whisper".into()], 10),
            Err(VoiceError::ExecutableNotAbsolute)
        ));
    }
    #[cfg(unix)]
    #[test]
    fn process_lives_for_held_interval_and_is_reaped() {
        let argv = vec![
            "/bin/sh".into(),
            "-c".into(),
            "read x || true; printf transcript".into(),
        ];
        let capture = VoiceCapture::start(&argv, 100).unwrap();
        assert!(capture.is_running());
        std::thread::sleep(Duration::from_millis(30));
        assert!(capture.is_running());
        assert_eq!(capture.finish(1000, 4).unwrap(), "tran");
    }

    #[cfg(unix)]
    #[test]
    fn dropping_live_capture_kills_and_reaps_process() {
        let argv = vec!["/bin/sh".into(), "-c".into(), "cat >/dev/null".into()];
        let capture = VoiceCapture::start(&argv, 100).unwrap();
        let pid = capture
            .child
            .lock()
            .expect("voice child poisoned")
            .as_ref()
            .expect("voice child exists")
            .id() as i32;
        drop(capture);
        let result = unsafe { libc::kill(pid, 0) };
        assert_eq!(result, -1, "voice child must not survive adapter drop");
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH),
            "voice child must be reaped, not left as a zombie"
        );
    }
    #[cfg(unix)]
    #[test]
    fn cancel_kills_and_reaps() {
        let argv = vec!["/bin/sh".into(), "-c".into(), "sleep 30".into()];
        let mut capture = VoiceCapture::start(&argv, 100).unwrap();
        assert!(capture.is_running());
        capture.cancel();
        assert!(!capture.is_running());
    }
}

//! Elevates commands through `sudo`, asking for the password via `askpass`.
//!
//! A small `askpass` shell script shows the password dialog with `osascript`
//! and is passed to `sudo` through `SUDO_ASKPASS`; the application never sees
//! or stores the password. Long running operations run from a temporary script
//! that redirects output to a log file which is tailed to produce live
//! progress.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crate::core::command::{Command, CommandRunner};
use crate::core::error::{CoreError, Result};

const SUDO: &str = "/usr/bin/sudo";
const OSASCRIPT: &str = "/usr/bin/osascript";
const POLL_INTERVAL: Duration = Duration::from_millis(120);
/// Number of trailing log lines included when a privileged command fails.
const FAILURE_LOG_LINES: usize = 20;

/// Wraps another runner, elevating commands marked [`Command::privileged`].
pub struct PrivilegedCommandRunner {
    inner: Box<dyn CommandRunner>,
    cancelled: AtomicBool,
    child: Mutex<Option<Child>>,
    pid_file: Mutex<Option<PathBuf>>,
    askpass: Mutex<Option<Askpass>>,
}

/// The temporary `SUDO_ASKPASS` helper; the directory keeps it alive.
struct Askpass {
    path: PathBuf,
    _dir: tempfile::TempDir,
}

impl PrivilegedCommandRunner {
    #[must_use]
    pub fn new(inner: Box<dyn CommandRunner>) -> Self {
        Self {
            inner,
            cancelled: AtomicBool::new(false),
            child: Mutex::new(None),
            pid_file: Mutex::new(None),
            askpass: Mutex::new(None),
        }
    }

    /// Path to the `SUDO_ASKPASS` helper, created on first use.
    fn askpass(&self) -> Result<PathBuf> {
        let mut guard = self.askpass.lock().unwrap();
        if let Some(askpass) = guard.as_ref() {
            return Ok(askpass.path.clone());
        }
        let dir =
            tempfile::TempDir::new().map_err(|error| CoreError::Privileged(error.to_string()))?;
        let path = dir.path().join("askpass.sh");
        fs::write(&path, askpass_script())
            .map_err(|error| CoreError::Privileged(error.to_string()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .map_err(|error| CoreError::Privileged(error.to_string()))?;
        *guard = Some(Askpass {
            path: path.clone(),
            _dir: dir,
        });
        Ok(path)
    }
}

impl Default for PrivilegedCommandRunner {
    fn default() -> Self {
        Self::new(Box::new(crate::core::command::SystemCommandRunner))
    }
}

impl CommandRunner for PrivilegedCommandRunner {
    fn run(&self, command: &Command) -> Result<String> {
        if !command.privileged {
            return self.inner.run(command);
        }
        let askpass = self.askpass()?;
        let sudo = Command::new(SUDO)
            .arg("-A")
            .arg(command.program.as_str())
            .args(command.args.iter().map(String::as_str))
            .env("SUDO_ASKPASS", askpass.display().to_string());
        self.inner.run(&sudo)
    }

    fn run_streaming(&self, command: &Command, on_line: &mut dyn FnMut(&str)) -> Result<()> {
        if !command.privileged {
            return self.inner.run_streaming(command, on_line);
        }

        let askpass = self.askpass()?;
        let dir =
            tempfile::TempDir::new().map_err(|error| CoreError::Privileged(error.to_string()))?;
        let script = dir.path().join("run.sh");
        let log = dir.path().join("out.log");
        let status_file = dir.path().join("status");
        let pid_file = dir.path().join("pid");

        fs::write(&script, script_body(command, &log, &status_file, &pid_file))
            .map_err(|error| CoreError::Privileged(error.to_string()))?;
        *self.pid_file.lock().unwrap() = Some(pid_file);

        let child = ProcessCommand::new(SUDO)
            .arg("-A")
            .arg("/bin/sh")
            .arg(&script)
            .env("SUDO_ASKPASS", &askpass)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| CoreError::Privileged(format!("failed to run sudo: {error}")))?;
        *self.child.lock().unwrap() = Some(child);

        let result = self.tail_log(&log, on_line);

        let child = self.child.lock().unwrap().take();
        let stderr = child.map(|mut child| {
            let mut buffer = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut buffer);
            }
            let _ = child.wait();
            buffer
        });
        *self.pid_file.lock().unwrap() = None;

        result?;

        let status_exists = status_file.exists();
        let status = fs::read_to_string(&status_file).unwrap_or_default();
        let code = status.trim().parse::<i32>().unwrap_or(1);
        if code == 0 && status_exists {
            return Ok(());
        }

        let sudo_stderr = stderr
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty());

        // No status file means the privileged shell never ran to completion:
        // either authorization was denied/cancelled, or `sudo` could not
        // elevate the command at all. sudo's stderr describes why.
        if !status_exists {
            let message = sudo_stderr.unwrap_or("administrator command failed");
            if is_cancelled(message) {
                return Err(CoreError::Cancelled);
            }
            return Err(CoreError::Privileged(message.to_owned()));
        }

        // The shell ran as root but a command inside it failed. Its output was
        // redirected to the log file, so surface the tail of that log rather
        // than sudo's stderr.
        let mut message = sudo_stderr.unwrap_or_default().to_owned();
        let tail = tail_file(&log, FAILURE_LOG_LINES);
        if !tail.trim().is_empty() {
            if !message.is_empty() {
                message.push('\n');
            }
            message.push_str(tail.trim_end());
        }
        if message.trim().is_empty() {
            message = format!("command exited with status {code}");
        }
        Err(CoreError::CommandFailed(message))
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(child) = self.child.lock().unwrap().as_mut() {
            let _ = child.kill();
        }
        if let Some(pid_file) = self.pid_file.lock().unwrap().as_ref() {
            if let Ok(pid) = fs::read_to_string(pid_file) {
                let pid = pid.trim();
                if !pid.is_empty() {
                    let _ = ProcessCommand::new("/bin/kill")
                        .args(["-TERM", pid])
                        .status();
                }
            }
        }
        self.inner.cancel();
    }
}

impl PrivilegedCommandRunner {
    /// Poll the redirected log file and forward complete lines.
    fn tail_log(&self, log: &Path, on_line: &mut dyn FnMut(&str)) -> Result<()> {
        let mut offset = 0u64;
        let mut pending = String::new();

        loop {
            if self.cancelled.load(Ordering::SeqCst) {
                return Err(CoreError::Cancelled);
            }
            if let Ok(mut file) = File::open(log) {
                if file.seek(SeekFrom::Start(offset)).is_ok() {
                    let mut chunk = String::new();
                    if file.read_to_string(&mut chunk).is_ok() {
                        offset += chunk.len() as u64;
                        pending.push_str(&chunk);
                        while let Some(index) = pending.find('\n') {
                            let line = pending[..index].to_owned();
                            pending.drain(..=index);
                            on_line(&line);
                        }
                    }
                }
            }

            let finished = {
                let mut guard = self.child.lock().unwrap();
                match guard.as_mut() {
                    Some(child) => match child.try_wait() {
                        Ok(Some(_)) => true,
                        Ok(None) => false,
                        Err(_) => true,
                    },
                    None => true,
                }
            };
            if finished {
                break;
            }
            std::thread::sleep(POLL_INTERVAL);
        }

        if !pending.trim().is_empty() {
            on_line(pending.trim_end());
        }
        Ok(())
    }
}

/// Read up to `max_lines` trailing lines of a log file, bounded in memory.
fn tail_file(path: &Path, max_lines: usize) -> String {
    const MAX_BYTES: u64 = 64 * 1024;
    let Ok(mut file) = File::open(path) else {
        return String::new();
    };
    let length = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    let start = length.saturating_sub(MAX_BYTES);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    let contents = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = contents.lines().collect();
    if lines.len() > max_lines {
        lines.drain(..lines.len() - max_lines);
    }
    lines.join("\n")
}

/// `SUDO_ASKPASS` helper: shows a hidden-answer dialog and prints the password.
fn askpass_script() -> String {
    use crate::core::command::shell_quote;
    let dialog = "display dialog \"Mac2Win Go needs your administrator password to write the \
                  drive.\" default answer \"\" with hidden answer with title \"Mac2Win Go\" \
                  buttons {\"OK\"} default button 1";
    format!(
        "#!/bin/sh\nexec {osascript} -e {dialog} -e 'text returned of result'\n",
        osascript = shell_quote(OSASCRIPT),
        dialog = shell_quote(dialog),
    )
}

/// Whether `sudo` failed because the user dismissed the password dialog.
fn is_cancelled(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("user canceled") || lower.contains("no password was provided")
}

/// Body of the temporary script used for streaming privileged commands.
fn script_body(command: &Command, log: &Path, status: &Path, pid: &Path) -> String {
    format!(
        "#!/bin/sh\necho $$ > {pid}\n{command} > {log} 2>&1\necho $? > {status}\n",
        pid = crate::core::command::shell_quote(&pid.display().to_string()),
        command = command.shell(),
        log = crate::core::command::shell_quote(&log.display().to_string()),
        status = crate::core::command::shell_quote(&status.display().to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quoting_handles_spaces_and_quotes() {
        use crate::core::command::shell_quote;
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("two words"), "'two words'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn askpass_shows_hidden_dialog() {
        let script = askpass_script();
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("/usr/bin/osascript"));
        assert!(script.contains("with hidden answer"));
        assert!(script.contains("text returned of result"));
    }

    #[test]
    fn cancellation_is_recognised() {
        assert!(is_cancelled("execution error: User canceled. (-128)"));
        assert!(is_cancelled("sudo: no password was provided"));
        assert!(!is_cancelled("sudo: 3 incorrect password attempts"));
    }

    #[test]
    fn script_body_redirects_output() {
        let command = Command::new("/usr/bin/true").arg("x");
        let body = script_body(
            &command,
            Path::new("/tmp/log"),
            Path::new("/tmp/status"),
            Path::new("/tmp/pid"),
        );
        assert!(body.contains("'/usr/bin/true' 'x' > '/tmp/log' 2>&1"));
        assert!(body.contains("echo $? > '/tmp/status'"));
    }

    #[test]
    fn delegates_unprivileged_commands() {
        use crate::core::command::SystemCommandRunner;
        let runner = PrivilegedCommandRunner::new(Box::new(SystemCommandRunner));
        let output = runner.run(&Command::new("/bin/echo").arg("ok")).unwrap();
        assert_eq!(output.trim(), "ok");
    }

    #[test]
    fn tail_file_keeps_last_lines() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("out.log");
        fs::write(&log, "one\ntwo\nthree\nfour\n").unwrap();
        assert_eq!(tail_file(&log, 2), "three\nfour");
        assert_eq!(tail_file(&log, 10), "one\ntwo\nthree\nfour");
    }

    #[test]
    fn tail_file_handles_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(tail_file(&dir.path().join("nope.log"), 5), "");
    }
}

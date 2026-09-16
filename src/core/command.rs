//! External command execution, abstracted so that the business logic can be
//! tested without touching the system.
//!
//! The real writer drives privileged tools (`diskutil`, `mkntfs`,
//! `wimlib-imagex`, ...) through a [`CommandRunner`]. Production uses
//! [`SystemCommandRunner`] (optionally wrapped by the platform's
//! `PrivilegedCommandRunner`), while tests inject a recording double.

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};

use super::error::{CoreError, Result};

/// POSIX single-quote escaping for embedding a value in a shell script.
#[must_use]
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Decode captured process output into text.
///
/// Most tools emit UTF-8, but some do not: `wimlib-imagex info --xml` dumps the
/// WIM's raw XML document, which is stored as UTF-16 with a byte-order mark.
/// Anything that is neither UTF-8 nor BOM-tagged UTF-16 is decoded lossily so a
/// stray byte never aborts an otherwise successful command.
#[must_use]
pub fn decode_output(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }

    let (utf16, little_endian) = if bytes.starts_with(&[0xFF, 0xFE]) {
        (&bytes[2..], true)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        (&bytes[2..], false)
    } else {
        return String::from_utf8_lossy(bytes).into_owned();
    };

    let units: Vec<u16> = utf16
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes(*pair)
            } else {
                u16::from_be_bytes(*pair)
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

/// A fully described external command invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub program: String,
    pub args: Vec<String>,
    /// Environment variables set for the spawned process only.
    pub env: Vec<(String, String)>,
    /// Whether the command must run with administrator privileges.
    pub privileged: bool,
    pub cwd: Option<PathBuf>,
}

impl Command {
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            privileged: false,
            cwd: None,
        }
    }

    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set an environment variable for just this command.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Mark this command as requiring root privileges.
    #[must_use]
    pub fn privileged(mut self) -> Self {
        self.privileged = true;
        self
    }

    #[must_use]
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Posix shell rendering, safe for embedding in a script.
    #[must_use]
    pub fn shell(&self) -> String {
        let mut rendered = shell_quote(&self.program);
        for arg in &self.args {
            rendered.push(' ');
            rendered.push_str(&shell_quote(arg));
        }
        rendered
    }

    /// Human readable rendering used in logs and error messages.
    #[must_use]
    pub fn display(&self) -> String {
        let mut rendered = self.program.clone();
        for arg in &self.args {
            rendered.push(' ');
            if arg.contains(char::is_whitespace) {
                rendered.push('"');
                rendered.push_str(arg);
                rendered.push('"');
            } else {
                rendered.push_str(arg);
            }
        }
        rendered
    }
}

/// Executes external commands.
pub trait CommandRunner: Send + Sync {
    /// Run to completion, returning captured stdout. Non-zero exit statuses and
    /// spawn failures are reported as [`CoreError`].
    fn run(&self, command: &Command) -> Result<String>;

    /// Run to completion, forwarding each stderr line to `on_line` as it is
    /// produced (most tools report progress on stderr).
    fn run_streaming(&self, command: &Command, on_line: &mut dyn FnMut(&str)) -> Result<()> {
        let output = self.run(command)?;
        for line in output.lines() {
            on_line(line);
        }
        Ok(())
    }

    /// Best-effort cancellation of an in-flight command.
    fn cancel(&self) {}
}

/// Runs commands on the real system.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommandRunner;

impl SystemCommandRunner {
    fn spawn(&self, command: &Command, capture_stderr: bool) -> Result<std::process::Child> {
        let mut process = ProcessCommand::new(&command.program);
        process.args(&command.args);
        process.envs(command.env.iter().map(|(key, value)| (key, value)));
        if let Some(cwd) = &command.cwd {
            process.current_dir(cwd);
        }
        if capture_stderr {
            process.stderr(Stdio::piped());
        }
        process.stdout(Stdio::piped());
        process.spawn().map_err(|error| {
            CoreError::Operation(format!("failed to run {}: {error}", command.display()))
        })
    }

    fn failure(command: &Command, stderr: &str) -> CoreError {
        let stderr = stderr.trim();
        if stderr.is_empty() {
            CoreError::Operation(format!("{} failed", command.display()))
        } else {
            CoreError::Operation(format!("{} failed: {stderr}", command.display()))
        }
    }
}

impl CommandRunner for SystemCommandRunner {
    fn run(&self, command: &Command) -> Result<String> {
        let child = self.spawn(command, true)?;
        let output = child
            .wait_with_output()
            .map_err(|error| CoreError::Operation(error.to_string()))?;

        if !output.status.success() {
            return Err(Self::failure(
                command,
                &String::from_utf8_lossy(&output.stderr),
            ));
        }

        Ok(decode_output(&output.stdout))
    }

    fn run_streaming(&self, command: &Command, on_line: &mut dyn FnMut(&str)) -> Result<()> {
        let mut child = self.spawn(command, true)?;

        let stdout = child.stdout.take();
        let stdout_handle = stdout.map(|stdout| {
            std::thread::spawn(move || {
                let mut buffer = String::new();
                let _ = BufReader::new(stdout).read_to_string(&mut buffer);
                buffer
            })
        });

        let mut captured = String::new();
        if let Some(stderr) = child.stderr.take() {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                on_line(&line);
                captured.push_str(&line);
                captured.push('\n');
            }
        }

        let status = child
            .wait()
            .map_err(|error| CoreError::Operation(error.to_string()))?;
        if let Some(handle) = stdout_handle {
            let _ = handle.join();
        }

        if !status.success() {
            return Err(Self::failure(command, &captured));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_quotes_arguments_with_spaces() {
        let command = Command::new("/bin/echo").arg("hello").arg("two words");
        assert_eq!(command.display(), "/bin/echo hello \"two words\"");
    }

    #[test]
    fn system_runner_returns_stdout() {
        let runner = SystemCommandRunner;
        let output = runner.run(&Command::new("/bin/echo").arg("hi")).unwrap();
        assert_eq!(output.trim(), "hi");
    }

    #[test]
    fn system_runner_reports_failure() {
        let runner = SystemCommandRunner;
        let error = runner
            .run(&Command::new("/bin/sh").arg("-c").arg("exit 3"))
            .unwrap_err();
        assert!(matches!(error, CoreError::Operation(_)));
    }

    #[test]
    fn decodes_utf16le_output() {
        // `wimlib-imagex info --xml` emits the WIM XML as UTF-16LE with a BOM.
        let bytes = b"\xFF\xFE<\0W\0I\0M\0>\0";
        assert_eq!(decode_output(bytes), "<WIM>");
    }

    #[test]
    fn decodes_utf16be_output() {
        let mut bytes = vec![0xFE, 0xFF];
        for unit in "<WIM>".encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        assert_eq!(decode_output(&bytes), "<WIM>");
    }

    #[test]
    fn decodes_utf8_and_falls_back_lossily() {
        assert_eq!(decode_output(b"plain"), "plain");
        assert_eq!(decode_output(&[0x66, 0x80, 0x6F]), "f\u{FFFD}o");
    }

    #[test]
    fn system_runner_decodes_utf16_stdout() {
        let runner = SystemCommandRunner;
        let output = runner
            .run(
                &Command::new("/bin/sh")
                    .arg("-c")
                    .arg("printf '\\377\\376<\\0W\\0I\\0M\\0>\\0'"),
            )
            .unwrap();
        assert_eq!(output, "<WIM>");
    }

    #[test]
    fn system_streaming_forwards_stderr_lines() {
        let runner = SystemCommandRunner;
        let mut lines = Vec::new();
        runner
            .run_streaming(
                &Command::new("/bin/sh")
                    .arg("-c")
                    .arg("echo one 1>&2; echo two 1>&2"),
                &mut |line| lines.push(line.to_owned()),
            )
            .unwrap();
        assert_eq!(lines, vec!["one".to_owned(), "two".to_owned()]);
    }
}

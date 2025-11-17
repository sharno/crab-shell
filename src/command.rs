use crate::{Error, Result, Shell};

use std::{
    ffi::OsString,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Command as StdCommand, ExitStatus, Output, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
};

/// Builder that mirrors `std::process::Command` but surfaces a friendlier API
/// tailored for composing pipelines.
#[derive(Debug, Clone)]
pub struct Command {
    program: OsString,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    clear_env: bool,
    current_dir: Option<PathBuf>,
    stdin: Option<Vec<u8>>,
    inherit_stdin: bool,
}

impl Command {
    /// Creates a new command. Use [`cmd`] for a terser helper.
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            clear_env: false,
            current_dir: None,
            stdin: None,
            inherit_stdin: false,
        }
    }

    /// Adds a single argument.
    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Extends the command with multiple arguments.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Sets/overrides an environment variable.
    pub fn env(
        mut self,
        key: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Clears the inherited environment before applying overrides.
    pub fn clear_env(mut self) -> Self {
        self.clear_env = true;
        self
    }

    /// Sets the working directory.
    pub fn current_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(dir.into());
        self
    }

    /// Feeds data into the command's stdin.
    pub fn stdin(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(data.into());
        self
    }

    /// Makes the process inherit the parent's stdin rather than capturing it.
    pub fn inherit_stdin(mut self, inherit: bool) -> Self {
        self.inherit_stdin = inherit;
        if !inherit {
            self.stdin = None;
        }
        self
    }

    /// Executes the command and returns its captured output.
    pub fn output(&self) -> Result<CommandOutput> {
        let std_output = self.spawn_and_wait()?;
        if !std_output.status.success() {
            return Err(Error::Command {
                program: self.program.clone(),
                status: std_output.status,
                stderr: String::from_utf8_lossy(&std_output.stderr)
                    .to_string(),
            });
        }
        Ok(CommandOutput {
            status: std_output.status,
            stdout: std_output.stdout,
            stderr: std_output.stderr,
        })
    }

    /// Runs the command, ignoring stdout/stderr, returning only the exit status.
    pub fn status(&self) -> Result<ExitStatus> {
        Ok(self.spawn_and_wait()?.status)
    }

    /// Returns the command stdout decoded as UTF-8 text.
    pub fn read(&self) -> Result<String> {
        Ok(self.output()?.stdout_string()?)
    }

    /// Returns stdout split by lines into a [`Shell`].
    pub fn lines(&self) -> Result<Shell<String>> {
        let text = self.read()?;
        let lines = text
            .lines()
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect::<Vec<_>>();
        Ok(Shell::from_iter(lines))
    }

    /// Creates a [`Pipeline`] with another command.
    pub fn pipe(self, next: Command) -> Pipeline {
        Pipeline::new(self, next)
    }

    /// Streams stdout line-by-line as the command executes.
    ///
    /// The resulting shell yields `Result<String>` so that consumers can surface
    /// non-zero exit statuses or read errors mid-stream.
    pub fn stream_lines(&self) -> Result<Shell<Result<String>>> {
        let mut command = self.build_std_command();
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let mut child = command.spawn()?;
        if let Some(input) = &self.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input)?;
            }
        }
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "missing stdout pipe",
            )))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "missing stderr pipe",
            )))?;
        let (tx, rx) = mpsc::channel();
        let program = self.program.clone();
        thread::spawn(move || {
            let stderr_handle = thread::spawn(move || -> String {
                let mut buf = String::new();
                let mut reader = BufReader::new(stderr);
                let _ = reader.read_to_string(&mut buf);
                buf
            });
            {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            let send_line =
                                line.trim_end_matches(&['\r', '\n'][..]).to_string();
                            if tx.send(Ok(send_line)).is_err() {
                                let _ = child.kill();
                                let _ = child.wait();
                                let _ = stderr_handle.join();
                                return;
                            }
                        }
                        Err(err) => {
                            let _ = tx.send(Err(Error::Io(err)));
                            let _ = child.kill();
                            let _ = child.wait();
                            let _ = stderr_handle.join();
                            return;
                        }
                    }
                }
            }
            let stderr_output = match stderr_handle.join() {
                Ok(buf) => buf,
                Err(_) => String::new(),
            };
            match child.wait() {
                Ok(status) => {
                    if !status.success() {
                        let _ = tx.send(Err(Error::Command {
                            program,
                            status,
                            stderr: stderr_output,
                        }));
                    }
                }
                Err(err) => {
                    let _ = tx.send(Err(Error::Io(err)));
                }
            }
        });
        Ok(Shell::new(ReceiverIter::new(rx)))
    }

    fn spawn_and_wait(&self) -> Result<Output> {
        let mut command = self.build_std_command();
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let mut child = command.spawn()?;
        if let Some(input) = &self.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input)?;
            }
        }
        Ok(child.wait_with_output()?)
    }

    fn build_std_command(&self) -> StdCommand {
        let mut command = StdCommand::new(&self.program);
        command.args(&self.args);
        if self.clear_env {
            command.env_clear();
        }
        command.envs(self.env.iter().cloned());
        if let Some(dir) = &self.current_dir {
            command.current_dir(dir);
        }
        if self.stdin.is_some() {
            command.stdin(Stdio::piped());
        } else if self.inherit_stdin {
            command.stdin(Stdio::inherit());
        }
        command
    }
}

/// Helper to create a [`Command`] from a program name.
pub fn cmd(program: impl Into<OsString>) -> Command {
    Command::new(program)
}

/// Executes a platform shell (`sh -c` or `cmd /C`).
pub fn sh(script: impl AsRef<str>) -> Command {
    let command = if cfg!(windows) {
        Command::new("cmd").arg("/C")
    } else {
        Command::new("sh").arg("-c")
    };
    command.arg(script.as_ref().to_string())
}

/// Output of a successfully executed command.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.status.success()
    }

    pub fn stdout_string(&self) -> Result<String> {
        Ok(String::from_utf8(self.stdout.clone())?)
    }

    pub fn stderr_string(&self) -> Result<String> {
        Ok(String::from_utf8(self.stderr.clone())?)
    }
}

/// Sequence of commands executed with stdout piped into the next stage.
#[derive(Debug, Clone)]
pub struct Pipeline {
    stages: Vec<Command>,
}

impl Pipeline {
    pub fn new(first: Command, second: Command) -> Self {
        Self {
            stages: vec![first, second],
        }
    }

    /// Adds another stage to the pipeline.
    pub fn pipe(mut self, next: Command) -> Self {
        self.stages.push(next);
        self
    }

    /// Executes the pipeline and returns the last stage's output.
    pub fn output(&self) -> Result<CommandOutput> {
        let mut input: Option<Vec<u8>> = None;
        let mut last = None;
        for stage in &self.stages {
            let mut stage = stage.clone();
            if let Some(stdin) = input.take() {
                stage = stage.stdin(stdin);
            }
            let output = stage.output()?;
            input = Some(output.stdout.clone());
            last = Some(output);
        }
        last.ok_or_else(|| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "empty pipeline",
            ))
        })
    }

    pub fn read(&self) -> Result<String> {
        Ok(self.output()?.stdout_string()?)
    }

    pub fn lines(&self) -> Result<Shell<String>> {
        let text = self.read()?;
        let lines = text
            .lines()
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect::<Vec<_>>();
        Ok(Shell::from_iter(lines))
    }
}

struct ReceiverIter<T> {
    rx: Receiver<T>,
}

impl<T> ReceiverIter<T> {
    fn new(rx: Receiver<T>) -> Self {
        Self { rx }
    }
}

impl<T> Iterator for ReceiverIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.rx.recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_lines_echoes() -> Result<()> {
        let cmd = sh("echo first && echo second");
        let lines: Result<Vec<_>> = cmd.stream_lines()?.collect();
        let lines = lines?;
        let cleaned: Vec<_> =
            lines.into_iter().map(|line| line.trim().to_string()).collect();
        assert_eq!(cleaned, vec!["first".to_string(), "second".to_string()]);
        Ok(())
    }

    #[test]
    fn pipeline_chains_basic_commands() -> Result<()> {
        let pipeline = sh("echo foo").pipe(sh("more"));
        let output = pipeline.read()?;
        assert!(output.to_lowercase().contains("foo"));
        Ok(())
    }
}

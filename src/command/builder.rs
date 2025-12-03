use crate::{Error, Result, Shell};

use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command as StdCommand, ExitStatus, Output, Stdio},
    sync::mpsc,
    thread,
};

#[cfg(feature = "async")]
use tokio::{io::AsyncWriteExt, process::Command as TokioCommand, task};

use super::{
    Pipeline, ReceiverIter, StdinJoinHandle, StdinSource, feed_child_stdin, wait_stdin_writer,
};

/// Alias to make builder intentions clearer in docs (`CommandBuilder` == [`Command`]).
#[allow(dead_code)]
pub type CommandBuilder = Command;

/// Builder that mirrors `std::process::Command` but surfaces a friendlier API
/// tailored for composing pipelines.
#[derive(Debug)]
pub struct Command {
    pub(crate) program: OsString,
    pub(crate) args: Vec<OsString>,
    pub(crate) env: Vec<(OsString, OsString)>,
    pub(crate) clear_env: bool,
    pub(crate) current_dir: Option<PathBuf>,
    pub(crate) stdin: Option<StdinSource>,
    pub(crate) inherit_stdin: bool,
}

impl Clone for Command {
    fn clone(&self) -> Self {
        Self {
            program: self.program.clone(),
            args: self.args.clone(),
            env: self.env.clone(),
            clear_env: self.clear_env,
            current_dir: self.current_dir.clone(),
            stdin: self.stdin.as_ref().and_then(StdinSource::try_clone),
            inherit_stdin: self.inherit_stdin,
        }
    }
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
    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
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
        self.stdin = Some(StdinSource::Bytes(data.into()));
        self.inherit_stdin = false;
        self
    }

    /// Streams from a reader without buffering all input.
    pub fn stdin_reader<R>(mut self, reader: R) -> Self
    where
        R: Read + Send + 'static,
    {
        self.stdin = Some(StdinSource::reader(reader));
        self.inherit_stdin = false;
        self
    }

    /// Makes the process inherit the parent's stdin rather than capturing it.
    pub fn inherit_stdin(mut self, inherit: bool) -> Self {
        self.inherit_stdin = inherit;
        if inherit {
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
                stderr: String::from_utf8_lossy(&std_output.stderr).to_string(),
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

    /// Runs the command while inheriting stdout/stderr from the parent process.
    pub fn run(&self) -> Result<()> {
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
        command.stdout(Stdio::inherit());
        command.stderr(Stdio::inherit());
        let mut child = command.spawn()?;
        let stdin_handle = feed_child_stdin(&mut child, &self.stdin)?;
        let status = child.wait()?;
        wait_stdin_writer(stdin_handle)?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Command {
                program: self.program.clone(),
                status,
                stderr: "stderr inherited by parent".into(),
            })
        }
    }

    /// Returns the command stdout decoded as UTF-8 text.
    pub fn stdout_text(&self) -> Result<String> {
        self.output()?.stdout_string()
    }

    /// Returns stdout split by lines into a [`Shell`].
    pub fn lines(&self) -> Result<Shell<String>> {
        let text = self.stdout_text()?;
        let lines = text
            .lines()
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect::<Vec<_>>();
        Ok(Shell::from_iter(lines))
    }

    /// Streams stderr line-by-line as the command executes.
    pub fn stream_stderr(&self) -> Result<Shell<Result<String>>> {
        let mut command = self.build_std_command();
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let stdin_handle = feed_child_stdin(&mut child, &self.stdin)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Io(std::io::Error::other("missing stdout pipe")))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Io(std::io::Error::other("missing stderr pipe")))?;
        let (tx, rx) = mpsc::channel();
        let program = self.program.clone();
        thread::spawn(move || {
            fn cleanup(child: &mut Child, stdin_handle: &mut Option<StdinJoinHandle>) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = wait_stdin_writer(stdin_handle.take());
            }
            let mut stdin_handle = stdin_handle;
            let stdout_handle = thread::spawn(move || -> String {
                let mut buf = String::new();
                let mut reader = BufReader::new(stdout);
                let _ = reader.read_to_string(&mut buf);
                buf
            });
            {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            let send_line = line.trim_end_matches(&['\r', '\n'][..]).to_string();
                            if tx.send(Ok(send_line)).is_err() {
                                cleanup(&mut child, &mut stdin_handle);
                                let _ = stdout_handle.join();
                                return;
                            }
                        }
                        Err(err) => {
                            let _ = tx.send(Err(Error::Io(err)));
                            cleanup(&mut child, &mut stdin_handle);
                            let _ = stdout_handle.join();
                            return;
                        }
                    }
                }
            }
            let stdout_output = stdout_handle.join().unwrap_or_default();
            let wait_result = child.wait();
            let stdin_result = wait_stdin_writer(stdin_handle);
            match wait_result {
                Ok(status) => {
                    if !status.success() {
                        let _ = stdin_result;
                        let _ = tx.send(Err(Error::Command {
                            program,
                            status,
                            stderr: stdout_output,
                        }));
                    } else if let Err(err) = stdin_result {
                        let _ = tx.send(Err(err));
                    }
                }
                Err(err) => {
                    let _ = stdin_result;
                    let _ = tx.send(Err(Error::Io(err)));
                }
            }
        });
        Ok(Shell::new(ReceiverIter::new(rx)))
    }

    /// Writes stdout to the specified file, replacing existing contents.
    pub fn write_stdout(&self, path: impl AsRef<Path>) -> Result<()> {
        let output = self.output()?;
        fs::write(path, &output.stdout)?;
        Ok(())
    }

    /// Appends stdout to the specified file.
    pub fn append_stdout(&self, path: impl AsRef<Path>) -> Result<()> {
        let output = self.output()?;
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(&output.stdout)?;
        Ok(())
    }

    /// Writes stdout to a file while still returning it to the caller.
    pub fn tee(&self, path: impl AsRef<Path>) -> Result<CommandOutput> {
        let output = self.output()?;
        fs::write(path, &output.stdout)?;
        Ok(output)
    }

    /// Writes stderr to a file while still returning captured output.
    pub fn tee_stderr(&self, path: impl AsRef<Path>) -> Result<CommandOutput> {
        let output = self.output()?;
        fs::write(path, &output.stderr)?;
        Ok(output)
    }

    /// Executes the command asynchronously (requires the `async` feature).
    #[cfg(feature = "async")]
    pub async fn output_async(&self) -> Result<CommandOutput> {
        if matches!(self.stdin.as_ref(), Some(StdinSource::Reader(_))) {
            return Err(Error::Io(std::io::Error::other(
                "stdin_reader is not supported in async mode",
            )));
        }
        let mut command = self.build_tokio_command();
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let mut child = command.spawn()?;
        if let Some(StdinSource::Bytes(input)) = &self.stdin
            && let Some(mut stdin) = child.stdin.take()
        {
            stdin.write_all(input).await?;
        }
        let output = child.wait_with_output().await?;
        if !output.status.success() {
            return Err(Error::Command {
                program: self.program.clone(),
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }
        Ok(CommandOutput {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    /// Runs the command asynchronously, inheriting parent stdio when configured.
    #[cfg(feature = "async")]
    pub async fn run_async(&self) -> Result<()> {
        self.output_async().await.map(|_| ())
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
        let stdin_handle = feed_child_stdin(&mut child, &self.stdin)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Io(std::io::Error::other("missing stdout pipe")))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Io(std::io::Error::other("missing stderr pipe")))?;
        let (tx, rx) = mpsc::channel();
        let program = self.program.clone();
        thread::spawn(move || {
            fn cleanup(
                child: &mut Child,
                stdin_handle: &mut Option<StdinJoinHandle>,
                stderr_handle: &mut Option<thread::JoinHandle<String>>,
            ) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = wait_stdin_writer(stdin_handle.take());
                if let Some(handle) = stderr_handle.take() {
                    let _ = handle.join();
                }
            }
            let mut stdin_handle = stdin_handle;
            let mut stderr_handle = Some(thread::spawn(move || -> String {
                let mut buf = String::new();
                let mut reader = BufReader::new(stderr);
                let _ = reader.read_to_string(&mut buf);
                buf
            }));
            {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            let send_line = line.trim_end_matches(&['\r', '\n'][..]).to_string();
                            if tx.send(Ok(send_line)).is_err() {
                                cleanup(&mut child, &mut stdin_handle, &mut stderr_handle);
                                return;
                            }
                        }
                        Err(err) => {
                            let _ = tx.send(Err(Error::Io(err)));
                            cleanup(&mut child, &mut stdin_handle, &mut stderr_handle);
                            return;
                        }
                    }
                }
            }
            let stderr_output = stderr_handle
                .take()
                .map(|h| h.join().unwrap_or_default())
                .unwrap_or_default();
            match child.wait() {
                Ok(status) => {
                    if !status.success() {
                        let _ = wait_stdin_writer(stdin_handle.take());
                        let _ = tx.send(Err(Error::Command {
                            program,
                            status,
                            stderr: stderr_output,
                        }));
                    } else if let Err(err) = wait_stdin_writer(stdin_handle.take()) {
                        let _ = tx.send(Err(err));
                    }
                }
                Err(err) => {
                    let _ = wait_stdin_writer(stdin_handle.take());
                    let _ = tx.send(Err(Error::Io(err)));
                }
            }
        });
        Ok(Shell::new(ReceiverIter::new(rx)))
    }

    /// Streams stdout asynchronously by delegating to the blocking implementation.
    #[cfg(feature = "async")]
    pub async fn stream_lines_async(&self) -> Result<Shell<Result<String>>> {
        let cmd = self.clone();
        let lines = task::spawn_blocking(move || {
            let shell = cmd.stream_lines()?;
            Ok::<Vec<Result<String>>, Error>(shell.collect())
        })
        .await
        .map_err(|err| {
            Error::Io(std::io::Error::other(format!(
                "stream task panicked: {err}"
            )))
        })??;
        Ok(Shell::from_iter(lines))
    }

    fn spawn_and_wait(&self) -> Result<Output> {
        let mut command = self.build_std_command();
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let stdin_handle = feed_child_stdin(&mut child, &self.stdin)?;
        let output = child.wait_with_output()?;
        wait_stdin_writer(stdin_handle)?;
        Ok(output)
    }

    fn build_std_command(&self) -> StdCommand {
        let mut command = StdCommand::new(&self.program);
        self.configure_std_command(&mut command);
        if self.stdin.is_some() {
            command.stdin(Stdio::piped());
        } else if self.inherit_stdin {
            command.stdin(Stdio::inherit());
        }
        command
    }

    pub(crate) fn configure_std_command(&self, command: &mut StdCommand) {
        command.args(&self.args);
        if self.clear_env {
            command.env_clear();
        }
        command.envs(self.env.iter().cloned());
        if let Some(dir) = &self.current_dir {
            command.current_dir(dir);
        }
    }

    #[cfg(feature = "async")]
    fn build_tokio_command(&self) -> TokioCommand {
        let mut command = TokioCommand::new(&self.program);
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

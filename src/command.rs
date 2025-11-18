use crate::{Error, Result, Shell};

use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command as StdCommand, ExitStatus, Output, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
};

#[cfg(feature = "async")]
use tokio::{io::AsyncWriteExt, process::Command as TokioCommand, task};

/// Alias to make builder intentions clearer in docs (`CommandBuilder` == [`Command`]).
#[allow(dead_code)]
pub type CommandBuilder = Command;

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
        self.stdin = Some(data.into());
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
        if let Some(input) = &self.stdin
            && let Some(mut stdin) = child.stdin.take()
        {
            stdin.write_all(input)?;
        }
        let status = child.wait()?;
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
    #[deprecated(note = "use `stdout_text` instead")]
    pub fn read(&self) -> Result<String> {
        self.stdout_text()
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
        if let Some(input) = &self.stdin
            && let Some(mut stdin) = child.stdin.take()
        {
            stdin.write_all(input)?;
        }
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
                                let _ = child.kill();
                                let _ = child.wait();
                                let _ = stdout_handle.join();
                                return;
                            }
                        }
                        Err(err) => {
                            let _ = tx.send(Err(Error::Io(err)));
                            let _ = child.kill();
                            let _ = child.wait();
                            let _ = stdout_handle.join();
                            return;
                        }
                    }
                }
            }
            let stdout_output = stdout_handle.join().unwrap_or_default();
            match child.wait() {
                Ok(status) => {
                    if !status.success() {
                        let _ = tx.send(Err(Error::Command {
                            program,
                            status,
                            stderr: stdout_output,
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
        let mut command = self.build_tokio_command();
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let mut child = command.spawn()?;
        if let Some(input) = &self.stdin
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

    /// Reads stdout asynchronously as UTF-8 text.
    #[cfg(feature = "async")]
    pub async fn read_async(&self) -> Result<String> {
        self.output_async().await?.stdout_string()
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
        if let Some(input) = &self.stdin
            && let Some(mut stdin) = child.stdin.take()
        {
            stdin.write_all(input)?;
        }
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
                            let send_line = line.trim_end_matches(&['\r', '\n'][..]).to_string();
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
            let stderr_output = stderr_handle.join().unwrap_or_default();
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
        if let Some(input) = &self.stdin
            && let Some(mut stdin) = child.stdin.take()
        {
            stdin.write_all(input)?;
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
        last.ok_or_else(|| Error::Io(std::io::Error::other("empty pipeline")))
    }

    #[deprecated(note = "use `stdout_text` instead")]
    pub fn read(&self) -> Result<String> {
        self.stdout_text()
    }

    pub fn stdout_text(&self) -> Result<String> {
        self.output()?.stdout_string()
    }

    /// Executes the pipeline ignoring stdout/stderr, returning only success.
    pub fn run(&self) -> Result<()> {
        self.output().map(|_| ())
    }

    pub fn lines(&self) -> Result<Shell<String>> {
        let text = self.stdout_text()?;
        let lines = text
            .lines()
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect::<Vec<_>>();
        Ok(Shell::from_iter(lines))
    }

    /// Writes the pipeline output to a file, overwriting existing contents.
    pub fn write_stdout(&self, path: impl AsRef<Path>) -> Result<()> {
        let output = self.output()?;
        fs::write(path, &output.stdout)?;
        Ok(())
    }

    /// Appends the pipeline output to a file.
    pub fn append_stdout(&self, path: impl AsRef<Path>) -> Result<()> {
        let output = self.output()?;
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(&output.stdout)?;
        Ok(())
    }

    /// Writes output to a file while returning the captured data.
    pub fn tee(&self, path: impl AsRef<Path>) -> Result<CommandOutput> {
        let output = self.output()?;
        fs::write(path, &output.stdout)?;
        Ok(output)
    }

    /// Writes stderr to a file while still returning the captured output.
    pub fn tee_stderr(&self, path: impl AsRef<Path>) -> Result<CommandOutput> {
        let output = self.output()?;
        fs::write(path, &output.stderr)?;
        Ok(output)
    }

    /// Streams stdout of the final pipeline stage line-by-line.
    pub fn stream_lines(&self) -> Result<Shell<Result<String>>> {
        if self.stages.is_empty() {
            return Err(Error::Io(std::io::Error::other("empty pipeline")));
        }
        let mut input: Option<Vec<u8>> = None;
        for (idx, stage) in self.stages.iter().enumerate() {
            let mut stage = stage.clone();
            if let Some(stdin) = input.take() {
                stage = stage.stdin(stdin);
            }
            let is_last = idx == self.stages.len() - 1;
            if is_last {
                return stage.stream_lines();
            }
            let output = stage.output()?;
            input = Some(output.stdout);
        }
        unreachable!("pipeline always has at least one stage")
    }

    /// Streams stderr of the final pipeline stage line-by-line.
    pub fn stream_stderr(&self) -> Result<Shell<Result<String>>> {
        if self.stages.is_empty() {
            return Err(Error::Io(std::io::Error::other("empty pipeline")));
        }
        let mut input: Option<Vec<u8>> = None;
        for (idx, stage) in self.stages.iter().enumerate() {
            let mut stage = stage.clone();
            if let Some(stdin) = input.take() {
                stage = stage.stdin(stdin);
            }
            let is_last = idx == self.stages.len() - 1;
            if is_last {
                return stage.stream_stderr();
            }
            let output = stage.output()?;
            input = Some(output.stdout);
        }
        unreachable!("pipeline always has at least one stage")
    }

    /// Streams stdout asynchronously by delegating to the blocking implementation.
    #[cfg(feature = "async")]
    pub async fn stream_lines_async(&self) -> Result<Shell<Result<String>>> {
        let pipe = self.clone();
        let lines = task::spawn_blocking(move || {
            let shell = pipe.stream_lines()?;
            Ok::<Vec<Result<String>>, Error>(shell.collect())
        })
        .await
        .map_err(|err| {
            Error::Io(std::io::Error::other(format!(
                "pipeline stream task panicked: {err}"
            )))
        })??;
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
    use tempfile::tempdir;

    fn stderr_command() -> Command {
        if cfg!(windows) {
            Command::new("cmd").arg("/C").arg("echo warn 1>&2")
        } else {
            Command::new("sh").arg("-c").arg("echo warn 1>&2")
        }
    }

    #[test]
    fn stream_lines_echoes() -> Result<()> {
        let cmd = sh("echo first && echo second");
        let lines: Result<Vec<_>> = cmd.stream_lines()?.collect();
        let lines = lines?;
        let cleaned: Vec<_> = lines
            .into_iter()
            .map(|line| line.trim().to_string())
            .collect();
        assert_eq!(cleaned, vec!["first".to_string(), "second".to_string()]);
        Ok(())
    }

    #[test]
    fn pipeline_stream_lines() -> Result<()> {
        let pipeline = sh("echo foo").pipe(sh("more"));
        let lines: Result<Vec<_>> = pipeline.stream_lines()?.collect();
        let lines = lines?;
        assert!(lines.iter().any(|line| line.to_lowercase().contains("foo")));
        Ok(())
    }

    #[test]
    fn stream_stderr_captures() -> Result<()> {
        let cmd = stderr_command();
        let lines: Result<Vec<_>> = cmd.stream_stderr()?.collect();
        let lines = lines?;
        assert!(
            lines
                .iter()
                .any(|line| line.to_lowercase().contains("warn"))
        );
        Ok(())
    }

    #[test]
    fn pipeline_stream_stderr() -> Result<()> {
        let pipeline = sh("echo hi").pipe(stderr_command());
        let lines: Result<Vec<_>> = pipeline.stream_stderr()?.collect();
        let lines = lines?;
        assert!(
            lines
                .iter()
                .any(|line| line.to_lowercase().contains("warn"))
        );
        Ok(())
    }

    #[test]
    fn pipeline_chains_basic_commands() -> Result<()> {
        let pipeline = sh("echo foo").pipe(sh("more"));
        let output = pipeline.stdout_text()?;
        assert!(output.to_lowercase().contains("foo"));
        Ok(())
    }

    #[test]
    fn run_inherits_stdio() {
        assert!(sh("exit 0").run().is_ok());
        assert!(sh("exit 1").run().is_err());
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn async_output_executes() -> Result<()> {
        let output = sh("echo async").output_async().await?;
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .to_lowercase()
                .contains("async")
        );
        Ok(())
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn async_stream_lines() -> Result<()> {
        let lines: Result<Vec<_>> = sh("echo a && echo b").stream_lines_async().await?.collect();
        let lines = lines?
            .into_iter()
            .map(|line| line.trim().to_string())
            .collect::<Vec<_>>();
        assert_eq!(lines, vec!["a".to_string(), "b".to_string()]);
        Ok(())
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn async_pipeline_stream_lines() -> Result<()> {
        let pipeline = sh("echo c && echo d").pipe(sh("more"));
        let lines: Result<Vec<_>> = pipeline.stream_lines_async().await?.collect();
        assert!(lines?.len() >= 2);
        Ok(())
    }

    #[test]
    fn tee_writes_files() -> Result<()> {
        let dir = tempdir()?;
        let file = dir.path().join("out.txt");
        let output = sh("echo hi").tee(&file)?;
        assert!(file.exists());
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .to_lowercase()
                .contains("hi")
        );

        let pipe_file = dir.path().join("pipe.txt");
        let pipeline = sh("echo hi").pipe(sh("more"));
        let output = pipeline.tee(&pipe_file)?;
        assert!(pipe_file.exists());
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .to_lowercase()
                .contains("hi")
        );

        let err_file = dir.path().join("err.txt");
        let err_output = stderr_command().tee_stderr(&err_file)?;
        assert!(err_file.exists());
        assert!(
            String::from_utf8_lossy(&err_output.stderr)
                .to_lowercase()
                .contains("warn")
        );
        Ok(())
    }
}

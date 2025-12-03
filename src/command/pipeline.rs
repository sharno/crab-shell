use crate::{Error, Result, Shell};

use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStderr, ChildStdout, Command as StdCommand, Stdio},
    sync::mpsc,
    thread,
};

#[cfg(feature = "async")]
use tokio::task;

use super::{
    Command, ReceiverIter, StdinJoinHandle, builder::CommandOutput, feed_child_stdin,
    wait_stdin_writer,
};

/// Sequence of commands executed with stdout piped into the next stage.
#[derive(Debug, Clone)]
pub struct Pipeline {
    stages: Vec<Command>,
}

#[derive(Debug)]
struct RunningStage {
    child: Child,
    program: OsString,
    stdin_handle: Option<thread::JoinHandle<std::io::Result<()>>>,
}

#[derive(Debug)]
struct FinalStage {
    child: Child,
    program: OsString,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    stdin_handle: Option<thread::JoinHandle<std::io::Result<()>>>,
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
        let (running, final_stage) = self.spawn_pipeline(true, true, false, false)?;
        let FinalStage {
            child,
            program,
            stdin_handle,
            ..
        } = final_stage;
        let output = child.wait_with_output()?;
        wait_stdin_writer(stdin_handle)?;
        wait_running_stages(running)?;
        if !output.status.success() {
            return Err(Error::Command {
                program,
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

    pub fn stdout_text(&self) -> Result<String> {
        self.output()?.stdout_string()
    }

    /// Executes the pipeline ignoring stdout/stderr, returning only success.
    pub fn run(&self) -> Result<()> {
        let (running, final_stage) = self.spawn_pipeline(false, false, false, false)?;
        let FinalStage {
            mut child,
            program,
            stdin_handle,
            ..
        } = final_stage;
        let status = child.wait()?;
        wait_stdin_writer(stdin_handle)?;
        let running_result = wait_running_stages(running);
        if !status.success() {
            let _ = running_result;
            return Err(Error::Command {
                program,
                status,
                stderr: "stderr inherited by parent".into(),
            });
        }
        running_result
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
        let (running, final_stage) = self.spawn_pipeline(true, true, true, true)?;
        let FinalStage {
            mut child,
            program,
            mut stdout,
            mut stderr,
            stdin_handle,
        } = final_stage;
        let stdout = stdout
            .take()
            .ok_or_else(|| Error::Io(std::io::Error::other("missing stdout pipe")))?;
        let stderr = stderr
            .take()
            .ok_or_else(|| Error::Io(std::io::Error::other("missing stderr pipe")))?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            fn cleanup(
                child: &mut Child,
                stdin_handle: &mut Option<StdinJoinHandle>,
                running: &mut Option<Vec<RunningStage>>,
                stderr_handle: &mut Option<thread::JoinHandle<String>>,
            ) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = wait_stdin_writer(stdin_handle.take());
                if let Some(handle) = stderr_handle.take() {
                    let _ = handle.join();
                }
                if let Some(stages) = running.take() {
                    let _ = wait_running_stages(stages);
                }
            }
            let mut stdin_handle = stdin_handle;
            let mut running = Some(running);
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
                                cleanup(
                                    &mut child,
                                    &mut stdin_handle,
                                    &mut running,
                                    &mut stderr_handle,
                                );
                                return;
                            }
                        }
                        Err(err) => {
                            if tx.send(Err(Error::Io(err))).is_err() {
                                cleanup(
                                    &mut child,
                                    &mut stdin_handle,
                                    &mut running,
                                    &mut stderr_handle,
                                );
                            }
                            return;
                        }
                    }
                }
            }
            let stderr_output = stderr_handle
                .take()
                .map(|h| h.join().unwrap_or_default())
                .unwrap_or_default();
            let wait_result = child.wait();
            let stdin_result = wait_stdin_writer(stdin_handle.take());
            let running_result = if let Some(stages) = running.take() {
                wait_running_stages(stages)
            } else {
                Ok(())
            };
            match wait_result {
                Ok(status) => {
                    if !status.success() {
                        let _ = stdin_result;
                        let _ = running_result;
                        let _ = tx.send(Err(Error::Command {
                            program,
                            status,
                            stderr: stderr_output,
                        }));
                        return;
                    }
                    if let Err(err) = stdin_result {
                        let _ = tx.send(Err(err));
                        return;
                    }
                    if let Err(err) = running_result {
                        let _ = tx.send(Err(err));
                    }
                }
                Err(err) => {
                    let _ = stdin_result;
                    let _ = running_result;
                    let _ = tx.send(Err(Error::Io(err)));
                }
            }
        });
        Ok(Shell::new(ReceiverIter::new(rx)))
    }

    /// Streams stderr of the final pipeline stage line-by-line.
    pub fn stream_stderr(&self) -> Result<Shell<Result<String>>> {
        let (running, final_stage) = self.spawn_pipeline(true, true, true, true)?;
        let FinalStage {
            mut child,
            program,
            mut stdout,
            mut stderr,
            stdin_handle,
        } = final_stage;
        let stdout = stdout
            .take()
            .ok_or_else(|| Error::Io(std::io::Error::other("missing stdout pipe")))?;
        let stderr = stderr
            .take()
            .ok_or_else(|| Error::Io(std::io::Error::other("missing stderr pipe")))?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            fn cleanup(
                child: &mut Child,
                stdin_handle: &mut Option<StdinJoinHandle>,
                running: &mut Option<Vec<RunningStage>>,
                stdout_handle: &mut Option<thread::JoinHandle<String>>,
            ) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = wait_stdin_writer(stdin_handle.take());
                if let Some(handle) = stdout_handle.take() {
                    let _ = handle.join();
                }
                if let Some(stages) = running.take() {
                    let _ = wait_running_stages(stages);
                }
            }
            let mut stdin_handle = stdin_handle;
            let mut running = Some(running);
            let mut stdout_handle = Some(thread::spawn(move || -> String {
                let mut buf = String::new();
                let mut reader = BufReader::new(stdout);
                let _ = reader.read_to_string(&mut buf);
                buf
            }));
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
                                cleanup(
                                    &mut child,
                                    &mut stdin_handle,
                                    &mut running,
                                    &mut stdout_handle,
                                );
                                return;
                            }
                        }
                        Err(err) => {
                            if tx.send(Err(Error::Io(err))).is_err() {
                                cleanup(
                                    &mut child,
                                    &mut stdin_handle,
                                    &mut running,
                                    &mut stdout_handle,
                                );
                            }
                            return;
                        }
                    }
                }
            }
            let stdout_output = stdout_handle
                .take()
                .map(|h| h.join().unwrap_or_default())
                .unwrap_or_default();
            let wait_result = child.wait();
            let stdin_result = wait_stdin_writer(stdin_handle.take());
            let running_result = if let Some(stages) = running.take() {
                wait_running_stages(stages)
            } else {
                Ok(())
            };
            match wait_result {
                Ok(status) => {
                    if !status.success() {
                        let _ = stdin_result;
                        let _ = running_result;
                        let _ = tx.send(Err(Error::Command {
                            program,
                            status,
                            stderr: stdout_output,
                        }));
                        return;
                    }
                    if let Err(err) = stdin_result {
                        let _ = tx.send(Err(err));
                        return;
                    }
                    if let Err(err) = running_result {
                        let _ = tx.send(Err(err));
                    }
                }
                Err(err) => {
                    let _ = stdin_result;
                    let _ = running_result;
                    let _ = tx.send(Err(Error::Io(err)));
                }
            }
        });
        Ok(Shell::new(ReceiverIter::new(rx)))
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

    fn spawn_pipeline(
        &self,
        capture_final_stdout: bool,
        capture_final_stderr: bool,
        take_final_stdout: bool,
        take_final_stderr: bool,
    ) -> Result<(Vec<RunningStage>, FinalStage)> {
        if self.stages.is_empty() {
            return Err(Error::Io(std::io::Error::other("empty pipeline")));
        }
        debug_assert!(!take_final_stdout || capture_final_stdout);
        debug_assert!(!take_final_stderr || capture_final_stderr);
        let mut previous_stdout: Option<ChildStdout> = None;
        let mut running = Vec::new();
        let last_idx = self.stages.len() - 1;
        for (idx, stage) in self.stages.iter().enumerate() {
            let mut command = StdCommand::new(&stage.program);
            stage.configure_std_command(&mut command);
            let mut uses_pipeline_input = false;
            if let Some(stdout) = previous_stdout.take() {
                command.stdin(Stdio::from(stdout));
                uses_pipeline_input = true;
            } else if stage.stdin.is_some() {
                command.stdin(Stdio::piped());
            } else if stage.inherit_stdin {
                command.stdin(Stdio::inherit());
            }

            let is_last = idx == last_idx;
            if is_last {
                if capture_final_stdout {
                    command.stdout(Stdio::piped());
                }
                if capture_final_stderr {
                    command.stderr(Stdio::piped());
                }
            } else {
                command.stdout(Stdio::piped());
                command.stderr(Stdio::inherit());
            }

            let mut child = command.spawn()?;
            let stdin_handle = if uses_pipeline_input {
                None
            } else {
                feed_child_stdin(&mut child, &stage.stdin)?
            };

            if is_last {
                let stdout_handle =
                    if take_final_stdout {
                        Some(child.stdout.take().ok_or_else(|| {
                            Error::Io(std::io::Error::other("missing stdout pipe"))
                        })?)
                    } else {
                        None
                    };
                let stderr_handle =
                    if take_final_stderr {
                        Some(child.stderr.take().ok_or_else(|| {
                            Error::Io(std::io::Error::other("missing stderr pipe"))
                        })?)
                    } else {
                        None
                    };
                return Ok((
                    running,
                    FinalStage {
                        child,
                        program: stage.program.clone(),
                        stdout: stdout_handle,
                        stderr: stderr_handle,
                        stdin_handle,
                    },
                ));
            }

            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| Error::Io(std::io::Error::other("missing stdout pipe")))?;
            previous_stdout = Some(stdout);
            running.push(RunningStage {
                child,
                program: stage.program.clone(),
                stdin_handle,
            });
        }

        unreachable!("pipeline must spawn at least one stage")
    }
}

fn wait_running_stages(stages: Vec<RunningStage>) -> Result<()> {
    for mut stage in stages {
        let status = stage.child.wait()?;
        wait_stdin_writer(stage.stdin_handle)?;
        if !status.success() {
            return Err(Error::Command {
                program: stage.program,
                status,
                stderr: "stderr inherited by parent".into(),
            });
        }
    }
    Ok(())
}

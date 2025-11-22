use super::*;
use crate::Result;
use std::io::Cursor;
use tempfile::tempdir;

fn noop_command() -> Command {
    if cfg!(windows) {
        Command::new("cmd").arg("/C").arg("exit 0")
    } else {
        Command::new("sh").arg("-c").arg(":")
    }
}

fn stderr_command() -> Command {
    if cfg!(windows) {
        Command::new("cmd").arg("/C").arg("echo warn 1>&2")
    } else {
        Command::new("sh").arg("-c").arg("echo warn 1>&2")
    }
}

fn stdin_passthrough_command() -> Command {
    if cfg!(windows) {
        Command::new("cmd").arg("/C").arg("more")
    } else {
        Command::new("cat")
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
fn stdin_reader_streams() -> Result<()> {
    let cursor = Cursor::new(b"stream-from-reader\n".to_vec());
    let output = stdin_passthrough_command()
        .stdin_reader(cursor)
        .stdout_text()?;
    assert!(output.contains("stream-from-reader"));
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
    let pipeline = noop_command().pipe(stderr_command());
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

#[test]
fn cloning_command_drops_stdin_reader() -> Result<()> {
    let reader_cmd = stdin_passthrough_command().stdin_reader(Cursor::new(b"data".to_vec()));
    let clone = reader_cmd.clone(); // stdin reader should not be carried over
    let output = reader_cmd.stdout_text()?;
    assert!(output.contains("data"));
    clone.inherit_stdin(true).run()?;
    Ok(())
}

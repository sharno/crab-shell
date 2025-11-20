use std::fs;

use qshr::prelude::*;
use tempfile::tempdir;

#[test]
fn command_stdin_reader_streams_large_input() -> qshr::Result<()> {
    let temp = tempdir()?;
    let source = temp.path().join("reader.txt");
    let data = (0..2048).map(|n| format!("line-{n}\n")).collect::<String>();
    write_text(&source, &data)?;

    let file = fs::File::open(&source)?;
    let output = cmd("wc").arg("-l").stdin_reader(file).stdout_text()?;
    assert_eq!(output.trim(), "2048");
    Ok(())
}

#[test]
fn pipeline_streams_and_tees_output() -> qshr::Result<()> {
    let temp = tempdir()?;
    let out = temp.path().join("pipeline.txt");

    let pipeline = sh("printf 'alpha\\nbeta\\n'").pipe(sh("grep alpha"));
    let lines: qshr::Result<Vec<_>> = pipeline.stream_lines()?.collect();
    assert_eq!(lines?.len(), 1);

    let capture = pipeline.tee(&out)?;
    assert!(capture.stdout_string()?.contains("alpha"));
    assert!(fs::read_to_string(&out)?.contains("alpha"));
    Ok(())
}

#[test]
fn command_streams_stderr() -> qshr::Result<()> {
    let lines: qshr::Result<Vec<_>> = cmd!("sh", "-c", "echo warn 1>&2")
        .stream_stderr()?
        .collect();
    let lines = lines?;
    assert!(lines.iter().any(|line| line.contains("warn")));
    Ok(())
}

#[test]
fn pipeline_run_propagates_failures() {
    let result = sh("false").pipe(sh("cat")).run();
    assert!(result.is_err());
}

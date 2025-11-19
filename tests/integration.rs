use std::{
    fs,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use qshr::{pipeline, prelude::*, qshr};
use tempfile::tempdir;

#[test]
fn macro_pipeline_integration() -> qshr::Result<()> {
    qshr! {
        let output = cmd!("sh", "-c", "echo pipeline").stdout_text()?;
        assert!(output.contains("pipeline"));
        "echo hi" | "wc -w";
    }?;
    Ok(())
}

#[test]
fn macro_pipeline_writes_files_and_restores_env() -> qshr::Result<()> {
    let temp = tempdir()?;
    let output = temp.path().join("macro.txt");

    qshr! {
        env "QSHR_IT_VAR" = "macro-integration";
        let capture = pipeline!(sh("echo macro integration flow") | "more");
        let contents = capture.stdout_text()?;
        write_text(&output, contents.trim().as_bytes())?;
        unset "QSHR_IT_VAR";
    }?;

    let written = fs::read_to_string(&output)?;
    assert!(written.to_lowercase().contains("macro integration flow"));
    assert!(std::env::var("QSHR_IT_VAR").is_err());
    Ok(())
}

#[test]
fn macro_watch_integration() -> qshr::Result<()> {
    let dir = tempdir()?;
    let file = dir.path().join("watch.txt");
    let dir_path = dir.path().to_path_buf();
    let hits = Mutex::new(Vec::new());
    qshr! {
        let events = watch_filtered(&dir_path, Duration::from_millis(150), "**/*.txt")?;
        let _ = thread::spawn({
            let file = file.clone();
            move || {
                thread::sleep(Duration::from_millis(50));
                let _ = std::fs::write(&file, b"event");
            }
        });
        for event in events.take(1) {
            let event = event?;
            hits.lock().unwrap().push(event.path().to_path_buf());
        }
    }?;
    assert_eq!(hits.lock().unwrap().as_slice(), &[file]);
    Ok(())
}

#[test]
fn watch_channel_reports_events_end_to_end() -> qshr::Result<()> {
    let temp = tempdir()?;
    let file = temp.path().join("watch.txt");
    let rx = watch_channel(temp.path())?;

    write_text(&file, "hello")?;

    let event = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("watch channel timed out")?;
    assert_eq!(event.path(), file.as_path());
    Ok(())
}

#[test]
fn macro_cd_parallel_integration() -> qshr::Result<()> {
    let temp = tempdir()?;
    let created = temp.path().join("parallel.txt");
    let hits = Arc::new(Mutex::new(Vec::new()));
    let hits_a = hits.clone();
    let hits_b = hits.clone();
    let original = std::env::current_dir()?;

    qshr! {
        cd(temp.path()) {
            write_text("parallel.txt", "inside cd block")?;
            parallel {
                let mut guard = hits_a.lock().unwrap();
                guard.push(String::from("left"));
            } {
                let mut guard = hits_b.lock().unwrap();
                guard.push(String::from("right"));
            };
        };
    }?;

    assert_eq!(std::env::current_dir()?, original);
    let contents = fs::read_to_string(&created)?;
    assert!(contents.contains("inside cd block"));
    assert_eq!(hits.lock().unwrap().len(), 2);
    Ok(())
}

#[test]
fn pipeline_macro_runs_via_run_helper() -> qshr::Result<()> {
    let temp = tempdir()?;
    let log = temp.path().join("log.txt");

    qshr! {
        let capture = pipeline!(cmd!("sh", "-c", "echo CAPTURED") | "tr A-Z a-z");
        let text = capture.stdout_text()?;
        assert_eq!(text.trim(), "captured");

        let append = pipeline!(cmd!("sh", "-c", &format!("echo run-helper >> \"{}\"", log.display())));
        run append;
    }?;

    let written = fs::read_to_string(&log)?;
    assert!(written.contains("run-helper"));
    Ok(())
}

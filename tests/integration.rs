use std::{sync::Mutex, thread, time::Duration};

use qshr::{prelude::*, qshr};
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

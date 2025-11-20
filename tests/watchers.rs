use std::{fs, thread, time::Duration};

use qshr::{Error, prelude::*};
use tempfile::tempdir;

fn next_event(
    mut events: Shell<qshr::Result<WatchEvent>>,
    predicate: impl Fn(&WatchEvent) -> bool,
    timeout: Duration,
) -> qshr::Result<WatchEvent> {
    let start = std::time::Instant::now();
    while let Some(event) = events.next() {
        let event = event?;
        if predicate(&event) {
            return Ok(event);
        }
        if start.elapsed() > timeout {
            break;
        }
    }
    Err(Error::Io(std::io::Error::other("watch event timeout")))
}

#[test]
fn watch_filtered_and_glob_emit_expected_events() -> qshr::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    let file = root.join("watch.txt");
    let filtered = watch_filtered(&root, Duration::from_millis(100), "**/*.txt")?;

    thread::spawn({
        let file = file.clone();
        move || {
            thread::sleep(Duration::from_millis(50));
            let _ = write_text(&file, "hello");
        }
    });

    let event = next_event(
        filtered,
        |event| matches!(event, WatchEvent::Created(entry) if entry.path == file),
        Duration::from_secs(2),
    )?;
    assert_eq!(event.path(), file.as_path());

    Ok(())
}

#[test]
fn watch_channel_reports_renames() -> qshr::Result<()> {
    let dir = tempdir()?;
    let from = dir.path().join("from.txt");
    let to = dir.path().join("to.txt");
    write_text(&from, "seed")?;
    let rx = watch_channel(dir.path())?;

    thread::sleep(Duration::from_millis(50));
    std::fs::rename(&from, &to)?;

    let mut seen = None;
    for _ in 0..10 {
        let event = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("watch rename timeout")?;
        if let WatchEvent::Renamed {
            from: old, to: new, ..
        } = event
        {
            seen = Some((old, new));
            break;
        }
    }
    let (old, new) = seen.expect("missing rename event");
    assert_eq!(old, from);
    assert_eq!(new, to);
    Ok(())
}

#[test]
fn debounce_watch_suppresses_duplicate_events() -> qshr::Result<()> {
    let dir = tempdir()?;
    let file = dir.path().join("debounce.txt");
    write_text(&file, "first")?;
    let metadata = fs::metadata(&file)?;
    let entry = PathEntry { path: file.clone(), metadata };
    let shell = Shell::from_iter(vec![
        Ok(WatchEvent::Created(entry.clone())),
        Ok(WatchEvent::Created(entry.clone())),
        Ok(WatchEvent::Created(entry)),
    ]);
    let deduped = debounce_watch(shell, Duration::from_millis(200))
        .collect::<qshr::Result<Vec<_>>>()?;
    assert_eq!(deduped.len(), 1);
    Ok(())
}

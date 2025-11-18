use qshr::prelude::*;
use std::time::Duration;

fn main() -> qshr::Result<()> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("input.txt");
    write_text(&file, "seed")?;

    println!("Watching {:?} for changes...", dir.path());
    let events = watch(dir.path(), Duration::from_millis(0), 1)?;
    for event in events {
        match event {
            WatchEvent::Created(entry) | WatchEvent::Modified(entry) => {
                println!("Change detected at {}", entry.path.display());
                sh("echo rebuilding").run()?;
            }
            WatchEvent::Removed(path) => {
                println!("Removed {}", path.display());
            }
        }
    }
    Ok(())
}

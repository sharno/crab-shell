use crab_shell::prelude::*;
use std::time::Duration;

fn main() -> crab_shell::Result<()> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("trigger.txt");

    // Simulate file creation/modification
    write_text(&file, "initial")?;

    let events = watch(dir.path(), Duration::from_millis(0), 1)?;
    for event in events {
        match event {
            WatchEvent::Created(entry) => {
                println!("Detected creation of {}", entry.path.display());
                sh("echo change detected").run()?;
            }
            WatchEvent::Modified(entry) => {
                println!("Modified {}", entry.path.display());
            }
            WatchEvent::Removed(path) => {
                println!("Removed {}", path.display());
            }
        }
    }

    Ok(())
}

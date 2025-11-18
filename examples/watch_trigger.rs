use qshr::prelude::*;

fn main() -> qshr::Result<()> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("trigger.txt");

    let events = watch(dir.path())?;
    write_text(&file, "initial")?;
    write_text(&file, "updated")?;
    rm(&file)?;

    for event in events.take(3) {
        match event? {
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

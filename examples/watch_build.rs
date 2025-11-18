use qshr::prelude::*;

fn main() -> qshr::Result<()> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("input.txt");

    println!("Watching {:?} for changes...", dir.path());
    let events = watch(dir.path())?;

    write_text(&file, "seed")?;
    write_text(&file, "next build")?;
    rm(&file)?;

    for event in events.take(3) {
        match event? {
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

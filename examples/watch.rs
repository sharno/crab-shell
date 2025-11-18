use qshr::prelude::*;
use std::time::Duration;

fn main() -> qshr::Result<()> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("watch.txt");

    let mut watcher = Watcher::new(dir.path())?;
    println!("Initial events: {:?}", watcher.poll()?);

    write_text(&file, "hello")?;
    println!("Created events: {:?}", watcher.poll()?);

    write_text(&file, "updated")?;
    println!("Modified events: {:?}", watcher.poll()?);

    rm(&file)?;
    println!("Removed events: {:?}", watcher.poll()?);

    println!("Running timed watcher...");
    let events: Vec<_> = watch(dir.path(), Duration::from_millis(0), 1)?.collect();
    println!("Timed events: {:?}", events);

    Ok(())
}

use qshr::{prelude::*, watch_glob};
use std::time::Duration;

fn main() -> qshr::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_path_buf();
    let file = root.join("match.rs");
    write_text(&file, "hello")?;
    write_text(&root.join("ignore.txt"), "skip")?;

    let events = watch(&root, Duration::from_millis(0), 1)?;
    for event in watch_glob(events, root.join("*.rs").to_string_lossy().as_ref())? {
        println!("glob matched event: {:?}", event.path());
    }
    Ok(())
}

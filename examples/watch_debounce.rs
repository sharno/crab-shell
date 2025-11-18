use qshr::{debounce_watch, prelude::*};
use std::time::Duration;

fn main() -> qshr::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_path_buf();
    let file = root.join("debounce.txt");
    write_text(&file, "hello")?;
    write_text(&file, "hello again")?;

    let events = watch(&root, Duration::from_millis(0), 2)?;
    for event in debounce_watch(events, Duration::from_millis(1)) {
        println!("debounced event: {:?}", event);
    }

    Ok(())
}

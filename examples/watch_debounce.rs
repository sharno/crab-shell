use qshr::{debounce_watch, prelude::*};
use std::time::Duration;

fn main() -> qshr::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_path_buf();
    let file = root.join("debounce.txt");
    let events = watch(&root)?;

    write_text(&file, "hello")?;
    write_text(&file, "hello again")?;

    for event in debounce_watch(events, Duration::from_millis(1)).take(2) {
        println!("debounced event: {:?}", event?);
    }

    Ok(())
}

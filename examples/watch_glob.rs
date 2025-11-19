use qshr::{prelude::*, watch_glob};

fn main() -> qshr::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_path_buf();
    let pattern = root.join("*.rs").to_string_lossy().to_string();

    let events = watch(&root)?;
    let file = root.join("match.rs");
    write_text(&file, "hello")?;
    write_text(root.join("ignore.txt"), "skip")?;

    for event in watch_glob(events, pattern)?.take(1) {
        println!("glob matched event: {:?}", event?.path());
    }
    Ok(())
}

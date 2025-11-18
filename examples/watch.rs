use qshr::prelude::*;

fn main() -> qshr::Result<()> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("watch.txt");

    let events = watch(dir.path())?;
    write_text(&file, "hello")?;
    write_text(&file, "updated")?;
    rm(&file)?;

    for event in events.take(3) {
        println!("watch event: {:?}", event?);
    }

    Ok(())
}

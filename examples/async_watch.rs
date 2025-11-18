#[cfg(feature = "async")]
use qshr::prelude::*;

#[cfg(feature = "async")]
#[tokio::main]
async fn main() -> qshr::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_path_buf();
    let file = root.join("async-watch.txt");
    write_text(&file, "alpha")?;

    let events = watch_async(root, std::time::Duration::from_millis(0), 1).await?;
    for event in events {
        println!("watch event: {:?}", event);
    }
    Ok(())
}

#[cfg(not(feature = "async"))]
fn main() -> qshr::Result<()> {
    println!("Enable the `async` feature to run this example.");
    Ok(())
}

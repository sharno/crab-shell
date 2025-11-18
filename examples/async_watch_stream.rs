#[cfg(feature = "async")]
use qshr::prelude::*;
#[cfg(feature = "async")]
use tokio_stream::StreamExt;

#[cfg(feature = "async")]
#[tokio::main]
async fn main() -> qshr::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_path_buf();
    let file = root.join("stream-watch.txt");
    write_text(&file, "alpha")?;

    let mut stream =
        watch_async_stream(root, std::time::Duration::from_millis(0), 1).await?;
    while let Some(event) = stream.next().await {
        println!("async watch event: {:?}", event?);
    }
    Ok(())
}

#[cfg(not(feature = "async"))]
fn main() -> qshr::Result<()> {
    println!("Enable the `async` feature to run this example.");
    Ok(())
}

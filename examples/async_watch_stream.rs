#[cfg(feature = "async")]
use qshr::prelude::*;
#[cfg(feature = "async")]
use tokio_stream::StreamExt;
#[cfg(feature = "async")]
use std::time::Duration;

#[cfg(feature = "async")]
#[tokio::main]
async fn main() -> qshr::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_path_buf();
    let file = root.join("stream-watch.txt");
    let writer = std::thread::spawn({
        let file = file.clone();
        move || {
            std::thread::sleep(Duration::from_millis(25));
            let _ = write_text(&file, "alpha");
            std::thread::sleep(Duration::from_millis(25));
            let _ = write_text(&file, "beta");
            let _ = rm(&file);
        }
    });

    let mut stream = watch_async_stream(root).await?.take(3);
    while let Some(event) = stream.next().await {
        println!("async watch event: {:?}", event?);
    }
    let _ = writer.join();
    Ok(())
}

#[cfg(not(feature = "async"))]
fn main() -> qshr::Result<()> {
    println!("Enable the `async` feature to run this example.");
    Ok(())
}

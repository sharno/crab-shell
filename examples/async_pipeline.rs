#[cfg(feature = "async")]
use qshr::prelude::*;
#[cfg(feature = "async")]
#[tokio::main]
async fn main() -> qshr::Result<()> {
    let pipeline = sh("echo alpha && echo beta").pipe(sh("more"));
    let lines: qshr::Result<Vec<_>> =
        pipeline.stream_lines_async().await?.collect();
    println!("lines: {:?}", lines?);
    Ok(())
}

#[cfg(not(feature = "async"))]
fn main() -> qshr::Result<()> {
    println!("Enable the `async` feature to run this example.");
    Ok(())
}

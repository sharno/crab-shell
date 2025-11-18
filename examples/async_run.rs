#[cfg(feature = "async")]
use qshr::prelude::*;
#[cfg(not(feature = "async"))]
use qshr::Result;

#[cfg(feature = "async")]
#[tokio::main]
async fn main() -> qshr::Result<()> {
    let output = sh("echo async example").output_async().await?;
    println!("stdout: {}", output.stdout_string()?);
    Ok(())
}

#[cfg(not(feature = "async"))]
fn main() -> Result<()> {
    println!("Enable the `async` feature to run this example.");
    Ok(())
}

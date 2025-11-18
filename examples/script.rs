use crab_shell::prelude::*;
use std::time::Duration;

fn main() -> crab_shell::Result<()> {
    println!("== checking versions ==");
    let rustc = cmd("rustc").arg("--version").read()?;
    println!("rustc: {}", rustc.trim());

    println!("== listing *.rs in src ==");
    for entry in watch_glob(watch("src", Duration::from_millis(0), 1)?, "src/*.rs")? {
        println!("- {}", entry.path().display());
    }

    println!("== piping command ==");
    let pipeline = sh("echo hello" ).pipe(sh("more"));
    for line in pipeline.stream_lines()? {
        println!("{}", line?);
    }

    Ok(())
}

use crab_shell::prelude::*;

fn main() -> crab_shell::Result<()> {
    let pipeline = sh("echo one && echo two").pipe(sh("more"));

    println!("Streaming stdout from pipeline:");
    for line in pipeline.stream_lines()? {
        let line = line?;
        println!("> {line}");
    }

    Ok(())
}

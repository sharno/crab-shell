use crab_shell::prelude::*;

fn main() -> crab_shell::Result<()> {
    println!("Listing current directory:");
    for path in ls(".")?.take(5) {
        println!(" - {}", path.display());
    }

    if let Some(home) = home_dir() {
        println!("Home dir -> {}", home.display());
    }

    let pipeline = sh("echo hello from crab-shell").pipe(sh("more"));
    println!("Pipeline said: {}", pipeline.read()?.trim());

    let temp = temp_file("crab-demo")?;
    write_text(&temp, "temporary scratch data")?;
    println!("Temp file created at {}", temp.display());
    rm(&temp)?;

    let rustc_version = cmd("rustc").arg("--version").read()?;
    println!("rustc -> {rustc_version}");
    Ok(())
}

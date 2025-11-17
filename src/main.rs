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
    let tee_path = temp_file("crab-pipeline")?;
    let pipeline_output = pipeline.tee(&tee_path)?;
    println!(
        "Pipeline said: {}",
        String::from_utf8_lossy(&pipeline_output.stdout).trim()
    );
    rm(&tee_path)?;

    let temp = temp_file("crab-demo")?;
    write_text(&temp, "temporary scratch data")?;
    println!("Temp file created at {}", temp.display());
    rm(&temp)?;

    println!("First few files rooted here:");
    let files: Vec<_> = walk_files(".")?.take(3).collect();
    for entry in &files {
        println!(" * {}", entry.path.display());
    }

    if !files.is_empty() {
        let mirror = temp_file("crab-copy")?;
        let mirror_dir = mirror.with_extension("dir");
        mkdir_all(&mirror_dir)?;
        copy_entries(
            Shell::from_iter(files.clone()),
            std::path::Path::new("."),
            &mirror_dir,
        )?;
        rm(&mirror_dir)?;
    }

    let rustc_version = cmd("rustc").arg("--version").read()?;
    println!("rustc -> {rustc_version}");
    Ok(())
}

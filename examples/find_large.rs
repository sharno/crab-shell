use crab_shell::prelude::*;

fn main() -> crab_shell::Result<()> {
    let pattern = "src/**/*.rs";
    let entries = glob_entries(pattern)?;

    let large_rs = filter_size(filter_extension(entries, "rs"), 1)
        .take(5)
        .collect::<Vec<_>>();

    println!("First {} Rust files (>=1 byte):", large_rs.len());
    for entry in large_rs {
        println!("{} bytes -> {}", entry.size(), entry.path.display());
    }

    Ok(())
}

use qshr::prelude::*;

fn main() -> qshr::Result<()> {
    let pattern = "src/**/*.rs";
    println!("Rust sources matching {pattern:?}:");

    let entries = glob_entries(pattern)?;
    let filtered = filter_extension(entries, "rs")
        .inspect(|entry| println!("  {}", entry.path.display()))
        .count();

    println!("Total files: {filtered}");
    Ok(())
}

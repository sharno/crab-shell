# Crab Shell 🦀

Crab Shell is a Rust-native take on the wonderful [Turtle](https://github.com/Gabriel439/Haskell-Turtle-Library) library from the Haskell ecosystem.  It provides:

- A lazy `Shell<T>` iterator with ergonomic combinators (`map`, `filter_map`, `chunks`, `windows`, `product`, …) for building expressive data pipelines.
- Optional parallel chunk processing when enabling the `parallel` feature (`Shell::chunk_map_parallel` is backed by `rayon`).
- A thin wrapper over `std::process::Command` and multi-stage `Pipeline`s that makes it easy to spawn commands, capture/stream both stdout and stderr, and redirect output to files.
- Battery-included filesystem helpers (directory walking, globbing with metadata, copying/moving trees, temporary files, directory watchers, etc.).
- A small set of environment helpers and a convenient prelude for `use crab_shell::prelude::*` workflows.

Everything is synchronous and depends only on the standard library plus `glob`/`tempfile` for a few utilities.

## Quick start

```rust
use crab_shell::prelude::*;

fn main() -> crab_shell::Result<()> {
    // Iterate over the first few files in the workspace
    for entry in walk_files(".")?.take(5) {
        println!("{}", entry.path.display());
    }

    // Build a streaming command just like Turtle
    sh("echo hello from crab-shell")
        .stream_lines()?
        .for_each(|line| println!("stdout: {line:?}"));

    // Capture stderr while also teeing it to a file
    let temp = temp_file("stderr-log")?;
    let cmd = sh("echo warn 1>&2");
    let stderr_lines: crab_shell::Result<Vec<_>> =
        cmd.stream_stderr()?.collect();
    cmd.tee_stderr(&temp)?;
    println!("stderr: {stderr_lines:?} (written to {})", temp.display());

    // Glob with metadata and filter by extension
    for entry in filter_extension(glob_entries("src/**/*.rs")?, "rs") {
        println!("source file: {}", entry.path.display());
    }

    Ok(())
}
```

## Additional examples

- `examples/glob_walk.rs` demonstrates metadata globbing with extension filtering.
- `examples/find_large.rs` filters globbed files by extension and size.
- `examples/pipeline_stream.rs` shows how to compose commands in a pipeline while streaming stdout incrementally.
- `examples/watch.rs` polls a directory for file creations, modifications, and removals.
- `examples/watch_trigger.rs` triggers a command (here `echo`) when file events occur.
- `examples/chunk_map_parallel.rs` demonstrates how to enable the `parallel` feature and process data chunks concurrently.

Run an example with `cargo run --example glob_walk`.

## Status

The crate is intentionally small and focused.  Contributions that keep the API ergonomic and dependency-light are welcome!  Ideas for future work include async variants, higher-level glob combinators, and more batteries for interacting with common system utilities.

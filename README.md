# Crab Shell

Crab Shell is a small Turtle-inspired toolkit for writing shell-style scripts in
Rust. A single `use crab_shell::prelude::*;` gives you:

- `Shell<T>`: a lazy iterator with handy combinators (`map`, `chunks`, `join`, ...).
- `Command`/`Pipeline`: an ergonomic wrapper around `std::process::Command`.
- Filesystem helpers: globbing, walking, copying, watchers, temp files, etc.

## Quick Examples

### 1. Stream a command
```rust
use crab_shell::prelude::*;

fn main() -> crab_shell::Result<()> {
    sh("echo hello && echo world")
        .stream_lines()?
        .for_each(|line| println!("stdout: {}", line?));
    Ok(())
}
```

### 2. Walk and filter files
```rust
use crab_shell::prelude::*;

fn main() -> crab_shell::Result<()> {
    let rust_sources = filter_extension(glob_entries("src/**/*.rs")?, "rs");
    for entry in rust_sources.take(3) {
        println!("{}", entry.path.display());
    }
    Ok(())
}
```

### 3. Rebuild when files change
```rust
use crab_shell::prelude::*;
use std::time::Duration;

fn main() -> crab_shell::Result<()> {
    let events = watch_filtered(
        ".",
        Duration::from_secs(1),
        10,
        Duration::from_millis(300),
        "**/*.rs",
    )?;

    for event in events {
        println!("changed: {}", event.path().display());
        sh("cargo check").run()?;
    }
    Ok(())
}
```

## Features

- `parallel`: enables `Shell::chunk_map_parallel` via `rayon`.
- `async`: exposes async helpers (e.g. `Command::output_async`,
  `watch_async_stream`) built on `tokio`.

## Examples

Browse `examples/` for small scripts—`script.rs`, `watch_glob.rs`,
`watch_debounce.rs`, the async runners, etc. Run them with
`cargo run --example <name>`.

## Status

The crate aims to stay compact and dependency-light. Contributions are welcome!

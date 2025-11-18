# Qshr (قِشر)

Qshr is a small Turtle-inspired toolkit for writing shell-style scripts in
Rust. A single `use qshr::prelude::*;` gives you:

- `Shell<T>`: a lazy iterator with handy combinators (`map`, `chunks`, `join`, ...).
- `Command`/`Pipeline`: an ergonomic wrapper around `std::process::Command`.
- Filesystem helpers: globbing, walking, copying, watchers, temp files, etc.

## Quick Examples

### 1. Stream a command

```rust
use qshr::prelude::*;

fn main() -> qshr::Result<()> {
    sh("echo hello && echo world")
        .stream_lines()?
        .for_each(|line| println!("stdout: {}", line?));
    Ok(())
}
```

### 2. Walk and filter files

```rust
use qshr::prelude::*;

fn main() -> qshr::Result<()> {
    let rust_sources = filter_extension(glob_entries("src/**/*.rs")?, "rs");
    for entry in rust_sources.take(3) {
        let entry = entry?;
        println!("{}", entry.path.display());
    }
    Ok(())
}
```

### 3. Rebuild when files change

```rust
use qshr::prelude::*;
use std::time::Duration;

fn main() -> qshr::Result<()> {
    let events = watch_filtered(".", Duration::from_millis(300), "**/*.rs")?;
    for event in events {
        let event = event?;
        println!("changed: {}", event.path().display());
        sh("cargo check").run()?;
    }
    Ok(())
}
```

### 4. Use the `qshr!` macro

```rust
use qshr::{prelude::*, qshr};

fn main() -> qshr::Result<()> {
    qshr! {
        println!("Running scripted commands...");
        "echo hi from macro";
        env "RUST_BACKTRACE" = "1";
        "echo RUST_BACKTRACE=$RUST_BACKTRACE";

        let rustc = cmd("rustc").arg("--version").read()?;
        println!("rustc -> {}", rustc.trim());

        "echo listing src" | "more";
        unset "RUST_BACKTRACE";
    }
}
```

String literals inside the macro run as shell commands automatically, and you can join them with `|` to build pipelines. Regular Rust statements (like the `let rustc = ...` line) work alongside the command sugar so you can still capture output or branch as needed. You can also set/unset environment variables inline with `env "KEY" = ...;` and `unset "KEY";`. See `examples/macro.rs` for the basics and `examples/macro_workflow.rs` for a more involved workflow.

#### Macro Patterns

```rust
use qshr::{prelude::*, qshr};

fn main() -> qshr::Result<()> {
    let tracked = ["src/lib.rs", "src/shell.rs"];
    qshr! {
        println!("Sanity checks");
        "cargo fmt";
        "cargo test --lib";

        for path in &tracked {
            let summary = cmd("wc").arg("-l").arg(path).read()?;
            print!("{summary}");
        };

        {
            let status = cmd("git").args(["status", "--short"]).read()?;
            println!("git status:\n{status}");
        };

        "rg TODO -n src" | "head -n 5";
    }?;
    Ok(())
}
```

Within `qshr!`, any Rust statement is permitted, so you can loop, branch, or shadow variables while the string literals do the repetitive shell work for you.

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

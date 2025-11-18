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

        let rustc = cmd("rustc").arg("--version").stdout_text()?;
        println!("rustc -> {}", rustc.trim());

        "echo listing src" | "more";
        cd("src") {
            "ls";
        };

        parallel {
            "echo one";
        } {
            "echo two";
        };

        unset "RUST_BACKTRACE";
    }
}
```

String literals inside the macro run as shell commands automatically, and you can join them with `|` to build pipelines. Regular Rust statements (like the `let rustc = ...` line) work alongside the command sugar so you can still capture output or branch as needed. You can also set/unset environment variables inline with `env "KEY" = ...;` and `unset "KEY";`, run blocks inside a different directory via `cd("path") { ... }`, and fire blocks in parallel threads with `parallel { ... } { ... };`. See `examples/macro.rs` for the basics and `examples/macro_workflow.rs` for a more involved workflow.

### 5. Build commands with `cmd!`

```rust
use qshr::{cmd, cmd as cmd_fn};

fn main() -> qshr::Result<()> {
    let output = cmd!("git", "status", "--short").stdout_text()?;
    println!("{output}");

    // Equivalent builder-style version.
    let fallback = cmd_fn("git").arg("status").arg("--short").stdout_text()?;
    assert_eq!(output, fallback);
    Ok(())
}
```

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
            let summary = cmd("wc").arg("-l").arg(path).stdout_text()?;
            print!("{summary}");
        };

        {
            let status = cmd("git").args(["status", "--short"]).stdout_text()?;
            println!("git status:\n{status}");
        };

        "rg TODO -n src" | "head -n 5";
    }?;
    Ok(())
}
```

Within `qshr!`, any Rust statement is permitted, so you can loop, branch, or shadow variables while the string literals do the repetitive shell work for you.

### 6. Lazy filesystem helpers

Every filesystem iterator (`ls`, `walk_files`, `glob_entries`, etc.) yields a `Shell<Result<_>>`, so you can lazily stream and short-circuit as needed:

```rust
use qshr::prelude::*;

fn main() -> qshr::Result<()> {
    let recent: Vec<_> = walk_files("src")?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry
                .modified()
                .ok()
                .and_then(|time| time.elapsed().ok())
                .filter(|age| *age.as_secs() < 300)
                .map(|_| entry)
        })
        .take(10)
        .collect();

    // When you want to collect fallible entries, use `collect::<qshr::Result<_>>()?`.
    let all: Vec<_> = walk_files("src")?.collect::<qshr::Result<Vec<_>>>()?;
    println!("First {} files, total {}.", recent.len(), all.len());
    Ok(())
}
```

## Usage patterns

### Pipelines with fallbacks

```rust
use qshr::prelude::*;

fn main() -> qshr::Result<()> {
    let rustc = cmd!("rustc", "--version").stdout_text()?;
    println!("rust -> {rustc}");

    // Pipe shell commands and capture the output.
    let files = sh("ls src").pipe(sh("wc -l")).stdout_text()?;
    println!("src has {files} entries");

    Ok(())
}
```

### Parallel chunk processing

```rust
use qshr::prelude::*;

fn main() -> qshr::Result<()> {
    let doubled: Vec<_> = Shell::from_iter(0..100)
        .chunks(16)
        .chunk_map_parallel(16, |chunk| chunk.into_iter().map(|n| n * 2).collect())
        .to_vec();
    println!("doubled len {}", doubled.len());
    Ok(())
}
```

### Watch and trigger work

See `examples/macro_watch.rs` for a `qshr!`-driven watcher; the core pattern is:

```rust
use qshr::prelude::*;
use std::time::Duration;

fn main() -> qshr::Result<()> {
    let events = watch_filtered(".", Duration::from_millis(300), "**/*.rs")?;
    for event in events.take(3) {
        let event = event?;
        println!("changed -> {}", event.path().display());
        sh("cargo check").run()?;
    }
    Ok(())
}

// Prefer a channel for manual polling:
let rx = watch_channel(".")?;
if let Ok(event) = rx.try_recv() {
    println!("changed -> {}", event?.path().display());
}
```

When you need to reuse glob metadata multiple times (copy/move operations, filtering), resolve once via `GlobCache::new("src/**/*.rs")` and call `.entries()` to avoid repeated `fs::metadata` calls.

Need backwards iteration? Wrap in `DoubleEndedShell::from_vec(vec)` and call `next_back()` on it before converting back into a plain `Shell`.

## Features

- `parallel`: enables `Shell::chunk_map_parallel` via `rayon`.
- `async`: exposes async helpers (e.g. `Command::output_async`,
  `watch_async_stream`) built on `tokio`.

## Examples

Browse `examples/` for small scripts—`script.rs`, `watch_glob.rs`,
`watch_debounce.rs`, the async runners, `macro_watch.rs`, etc. Run them with
`cargo run --example <name>`.

## Status

The crate aims to stay compact and dependency-light. Contributions are welcome!

## Submodules

This repository vendors the original Haskell [`turtle`](https://github.com/Gabriella439/turtle)
project as a Git submodule (`turtle/`). After cloning, make sure to run:

```
git submodule update --init
```

so the submodule is checked out locally.

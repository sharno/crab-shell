//! Crab Shell - a Turtle-inspired ergonomic shell toolkit for Rust.
//!
//! The crate pairs a lazy [`Shell`] iterator abstraction with a small command
//! runner that provides composable pipelines reminiscent of the Haskell Turtle
//! library.  Everything is synchronous, deterministic, and built on top of the
//! Rust standard library to keep the dependency footprint tiny.

mod command;
mod error;
mod fs;
mod env;
mod shell;

pub mod prelude;

pub use command::{cmd, sh, Command, CommandOutput, Pipeline};
pub use error::{Error, Result};
pub use env::*;
pub use fs::{
    append_text, cat, copy_dir, copy_entries, copy_file, filter_extension,
    filter_modified_since, filter_size, glob, glob_entries, ls, ls_detailed,
    mkdir_all, move_path, read_lines, read_text, rm, temp_file, watch, walk,
    walk_detailed, walk_files, walk_filter, write_lines, write_text, PathEntry,
    WatchEvent, Watcher,
};
pub use shell::Shell;

/// Convenience module with the most frequently used items.
///
/// ```no_run
/// use crab_shell::prelude::*;
///
/// fn main() -> crab_shell::Result<()> {
///     for path in ls(".")? {
///         println!("{}", path.display());
///     }
///
///     let lines = cmd("cargo").arg("--version").lines()?;
///     for line in lines {
///         println!("cargo: {line}");
///     }
///
///     Ok(())
/// }
/// ```

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_basic_map() {
        let mapped: Vec<_> =
            Shell::from_iter([1, 2, 3]).map(|n| n * n).collect();
        assert_eq!(mapped, vec![1, 4, 9]);
    }
}

mod entries;
mod filter;
mod glob;
mod io;
mod walk;
mod watch;

pub use entries::PathEntry;
pub use filter::{filter_extension, filter_modified_since, filter_size};
pub use glob::{glob, glob_entries, GlobCache};
pub use glob::watch_glob;
pub use io::{
    append_text, cat, copy_dir, copy_entries, copy_file, mkdir_all, move_path, read_lines,
    read_text, rm, temp_file, write_lines, write_text,
};
pub use walk::{ls, ls_detailed, walk, walk_detailed, walk_files, walk_filter};
pub use watch::{
    WatchEvent, Watcher, debounce_watch, watch, watch_channel, watch_filtered,
};
#[cfg(feature = "async")]
pub use watch::{watch_async, watch_async_stream, watch_filtered_async};

#[cfg(test)]
mod tests;

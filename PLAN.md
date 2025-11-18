## Qshr Improvement Plan

### Filesystem & Streaming

- Ensure all filesystem helpers return `Shell<Result<_>>` lazily; remove lingering eager `Vec` collections and document the pattern (`collect::<Result<_>>()?`).
- Rework `read_lines`/`cat` to stream via iterator adapters instead of buffering entire files, keeping memory flat for large inputs.

### Command/Pipeline API

- Break compatibility to rename `Command::read`/`Pipeline::read` to clearer names (`stdout_text` or similar) and have `run` return `ExitStatus` for better inspection.
- Provide a `cmd!` macro for literal commands that builds `Command` instances with less boilerplate.

### Shell Core Improvements

- Implement iterator traits (`ExactSizeIterator`, `DoubleEndedIterator`) where applicable so adapters compose better.
- Optimize `chunk_map` to avoid collecting entire datasets when `chunk_size` is large, possibly by using a streaming buffer.

### Docs & Examples

- Expand macro documentation with real-world workflows (`examples/macro_workflow.rs`, potential `macro_watch.rs`).
- Add a “patterns” section for pipelines, parallel chunking, watchers, etc., so users can copy/paste common scripts.

### Performance/Robustness

- Add a non-blocking watcher option that spawns `notify` on its own thread, allowing `watch()` to be dropped cleanly.
- Cache glob metadata in `glob_entries` to avoid repeated `fs::metadata` calls when traversing large trees.

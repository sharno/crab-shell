## Qshr Improvement Plan

### Shell Core Improvements

- Implement iterator traits (`ExactSizeIterator`, `DoubleEndedIterator`) where applicable so adapters compose better.
### Docs & Examples

- Expand macro documentation with real-world workflows (`examples/macro_workflow.rs`, potential `macro_watch.rs`).
- Add a “patterns” section for pipelines, parallel chunking, watchers, etc., so users can copy/paste common scripts.

### Performance/Robustness

- Add a non-blocking watcher option that spawns `notify` on its own thread, allowing `watch()` to be dropped cleanly.
- Cache glob metadata in `glob_entries` to avoid repeated `fs::metadata` calls when traversing large trees.

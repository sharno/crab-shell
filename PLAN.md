## Qshr Improvement Plan

### Shell Core Improvements

- Implement iterator traits (`ExactSizeIterator`, `DoubleEndedIterator`) where applicable so adapters compose better.
### Testing
### Performance/Robustness

- Cache glob metadata in `glob_entries` to avoid repeated `fs::metadata` calls when traversing large trees.

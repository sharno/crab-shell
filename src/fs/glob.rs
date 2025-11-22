use crate::{Result, Shell};

use std::fs;

use glob::{Pattern, glob as glob_iter};

use super::entries::PathEntry;

/// Expands filesystem globs (e.g. `*.rs`) into a stream of paths.
pub fn glob(pattern: impl AsRef<str>) -> Result<Shell<Result<std::path::PathBuf>>> {
    let iter = glob_iter(pattern.as_ref())?;
    Ok(Shell::new(Box::new(
        iter.map(|entry| entry.map_err(Into::into)),
    )))
}

/// Expands globs while returning [`PathEntry`] metadata.
pub fn glob_entries(pattern: impl AsRef<str>) -> Result<Shell<Result<PathEntry>>> {
    let iter = glob_iter(pattern.as_ref())?;
    Ok(Shell::new(Box::new(iter.map(|entry| {
        let path = entry?;
        let metadata = fs::metadata(&path)?;
        Ok(PathEntry { path, metadata })
    }))))
}

/// Cached glob results for reuse across multiple operations.
#[derive(Debug, Clone)]
pub struct GlobCache {
    entries: Vec<PathEntry>,
}

impl GlobCache {
    /// Resolves `pattern` immediately, storing `PathEntry` data in memory.
    pub fn new(pattern: impl AsRef<str>) -> Result<Self> {
        let entries = glob_entries(pattern)?.collect::<Result<Vec<_>>>()?;
        Ok(Self { entries })
    }

    /// Returns the cached entries.
    pub fn entries(&self) -> &[PathEntry] {
        &self.entries
    }

    /// Consumes the cache, returning owned entries.
    pub fn into_entries(self) -> Vec<PathEntry> {
        self.entries
    }
}

/// Filters watch events by glob pattern (case-sensitive).
pub fn watch_glob(
    events: Shell<Result<super::watch::WatchEvent>>,
    pattern: impl AsRef<str>,
) -> Result<Shell<Result<super::watch::WatchEvent>>> {
    let pattern = Pattern::new(pattern.as_ref())?;
    Ok(events.filter(move |event| match event {
        Ok(event) => pattern.matches_path(event.path()),
        Err(_) => true,
    }))
}

use crate::{Result, Shell};

use std::time::SystemTime;

use super::entries::PathEntry;

/// Filters entries to only those matching the provided extension (case-insensitive).
pub fn filter_extension(
    entries: Shell<Result<PathEntry>>,
    ext: impl AsRef<str>,
) -> Shell<Result<PathEntry>> {
    let needle = ext.as_ref().to_ascii_lowercase();
    entries.filter_map(move |entry| match entry {
        Ok(entry) => entry
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase() == needle)
            .unwrap_or(false)
            .then_some(Ok(entry)),
        Err(err) => Some(Err(err)),
    })
}

/// Keeps entries at or above the specified size (in bytes).
pub fn filter_size(entries: Shell<Result<PathEntry>>, min_bytes: u64) -> Shell<Result<PathEntry>> {
    entries.filter_map(move |entry| match entry {
        Ok(entry) => (entry.size() >= min_bytes).then_some(Ok(entry)),
        Err(err) => Some(Err(err)),
    })
}

/// Keeps entries modified at or after `since`.
pub fn filter_modified_since(
    entries: Shell<Result<PathEntry>>,
    since: SystemTime,
) -> Shell<Result<PathEntry>> {
    entries.filter_map(move |entry| match entry {
        Ok(entry) => entry
            .modified()
            .map(|time| time >= since)
            .unwrap_or(false)
            .then_some(Ok(entry)),
        Err(err) => Some(Err(err)),
    })
}

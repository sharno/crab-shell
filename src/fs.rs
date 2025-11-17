use crate::{Result, Shell};

use std::{
    collections::HashMap,
    env,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use glob::glob as glob_iter;

/// Metadata about a filesystem path captured during listing operations.
#[derive(Debug, Clone)]
pub struct PathEntry {
    pub path: PathBuf,
    pub metadata: fs::Metadata,
}

impl PathEntry {
    pub fn is_dir(&self) -> bool {
        self.metadata.is_dir()
    }

    pub fn is_file(&self) -> bool {
        self.metadata.is_file()
    }

    pub fn file_name(&self) -> Option<&OsStr> {
        self.path.file_name()
    }

    pub fn extension(&self) -> Option<&OsStr> {
        self.path.extension()
    }

    pub fn size(&self) -> u64 {
        self.metadata.len()
    }

    pub fn modified(&self) -> Option<SystemTime> {
        self.metadata.modified().ok()
    }
}

impl PartialEq for PathEntry {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
            && self.size() == other.size()
            && self.is_dir() == other.is_dir()
            && self.modified() == other.modified()
    }
}

impl Eq for PathEntry {}

/// Lists the immediate children of a directory.
pub fn ls(path: impl AsRef<Path>) -> Result<Shell<PathBuf>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        entries.push(entry.path());
    }
    Ok(Shell::from_iter(entries))
}

/// Lists the immediate children of a directory, including metadata.
pub fn ls_detailed(path: impl AsRef<Path>) -> Result<Shell<PathEntry>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        entries.push(PathEntry {
            path: entry.path(),
            metadata,
        });
    }
    Ok(Shell::from_iter(entries))
}

/// Recursively walks the directory tree depth-first including the root.
pub fn walk(root: impl AsRef<Path>) -> Result<Shell<PathBuf>> {
    let mut stack = vec![root.as_ref().to_path_buf()];
    let mut acc = Vec::new();

    while let Some(path) = stack.pop() {
        acc.push(path.clone());
        if path.is_dir() {
            for entry in fs::read_dir(&path)? {
                let entry = entry?;
                stack.push(entry.path());
            }
        }
    }

    Ok(Shell::from_iter(acc))
}

/// Recursively walks the directory tree, including metadata for each entry.
pub fn walk_detailed(root: impl AsRef<Path>) -> Result<Shell<PathEntry>> {
    let mut stack = vec![root.as_ref().to_path_buf()];
    let mut acc = Vec::new();

    while let Some(path) = stack.pop() {
        let metadata = fs::metadata(&path)?;
        let is_dir = metadata.is_dir();
        acc.push(PathEntry {
            path: path.clone(),
            metadata,
        });
        if is_dir {
            for entry in fs::read_dir(&path)? {
                let entry = entry?;
                stack.push(entry.path());
            }
        }
    }

    Ok(Shell::from_iter(acc))
}

/// Walks the tree and yields only file entries.
pub fn walk_files(root: impl AsRef<Path>) -> Result<Shell<PathEntry>> {
    Ok(walk_detailed(root)?.filter(|entry| entry.is_file()))
}

/// Walks the tree and keeps entries matching the predicate.
pub fn walk_filter<F>(
    root: impl AsRef<Path>,
    predicate: F,
) -> Result<Shell<PathEntry>>
where
    F: FnMut(&PathEntry) -> bool + 'static,
{
    Ok(walk_detailed(root)?.filter(predicate))
}

/// Reads a UTF-8 file completely into a `String`.
pub fn read_text(path: impl AsRef<Path>) -> Result<String> {
    Ok(fs::read_to_string(path)?)
}

/// Reads a file as a stream of lines.
pub fn read_lines(path: impl AsRef<Path>) -> Result<Shell<String>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    for line in reader.lines() {
        lines.push(line?);
    }
    Ok(Shell::from_iter(lines))
}

/// Writes the provided text to the path (truncating existing file).
pub fn write_text(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    fs::write(path, contents)?;
    Ok(())
}

/// Writes newline separated lines to a file.
pub fn write_lines(
    path: impl AsRef<Path>,
    lines: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<()> {
    let mut file = File::create(path)?;
    for line in lines {
        file.write_all(line.as_ref().as_bytes())?;
        file.write_all(b"\n")?;
    }
    Ok(())
}

/// Copies a file from `from` to `to`.
pub fn copy_file(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let _ = fs::copy(from, to)?;
    Ok(())
}

/// Appends bytes to the end of the given file, creating it if needed.
pub fn append_text(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(contents.as_ref())?;
    Ok(())
}

/// Concatenates multiple files line-by-line.
pub fn cat<P, I>(paths: I) -> Result<Shell<String>>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = P>,
{
    let mut out = Vec::new();
    for path in paths {
        let file = File::open(path.as_ref())?;
        for line in BufReader::new(file).lines() {
            out.push(line?);
        }
    }
    Ok(Shell::from_iter(out))
}

/// Creates a directory and all missing parents.
pub fn mkdir_all(path: impl AsRef<Path>) -> Result<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

/// Removes a file or directory tree.
pub fn rm(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if path.is_dir() {
        fs::remove_dir_all(path)?;
    } else if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Recursively copies a directory tree.
pub fn copy_dir(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    mkdir_all(to)?;
    let mut walker = walk(from)?;
    while let Some(path) = walker.next() {
        let relative = path.strip_prefix(from).unwrap_or(&path);
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = to.join(relative);
        if path.is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

/// Moves a file or directory, falling back to copy/remove when needed.
pub fn move_path(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    match fs::rename(from, to) {
        Ok(_) => Ok(()),
        Err(_) => {
            if from.is_dir() {
                copy_dir(from, to)?;
                rm(from)?;
            } else {
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(from, to)?;
                fs::remove_file(from)?;
            }
            Ok(())
        }
    }
}

/// Copies files yielded by `entries` into `destination`, preserving relative paths.
pub fn copy_entries(
    entries: Shell<PathEntry>,
    root: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> Result<()> {
    let root = root.as_ref();
    let destination = destination.as_ref();
    for entry in entries {
        let relative = entry.path.strip_prefix(root).unwrap_or(&entry.path);
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&entry.path, &target)?;
        }
    }
    Ok(())
}

/// File system change events emitted by [`Watcher`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    Created(PathEntry),
    Modified(PathEntry),
    Removed(PathBuf),
}

/// Simple polling watcher that diffs directory snapshots.
pub struct Watcher {
    root: PathBuf,
    snapshot: HashMap<PathBuf, PathEntry>,
}

impl Watcher {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let snapshot = snapshot_dir(&root)?;
        Ok(Self { root, snapshot })
    }

    /// Polls for changes since the last call.
    pub fn poll(&mut self) -> Result<Vec<WatchEvent>> {
        let current = snapshot_dir(&self.root)?;
        let events = diff_snapshots(&self.snapshot, &current);
        self.snapshot = current;
        Ok(events)
    }
}

/// Convenience helper that polls a directory on a fixed interval and collects events.
pub fn watch(
    root: impl AsRef<Path>,
    interval: Duration,
    iterations: usize,
) -> Result<Shell<WatchEvent>> {
    let mut watcher = Watcher::new(root)?;
    let mut events = Vec::new();
    for _ in 0..iterations {
        if interval > Duration::from_millis(0) {
            thread::sleep(interval);
        }
        events.extend(watcher.poll()?);
    }
    Ok(Shell::from_iter(events))
}

fn snapshot_dir(root: &Path) -> Result<HashMap<PathBuf, PathEntry>> {
    let mut map = HashMap::new();
    if !root.exists() {
        return Ok(map);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        map.insert(
            entry.path(),
            PathEntry {
                path: entry.path(),
                metadata: entry.metadata()?,
            },
        );
    }
    Ok(map)
}

fn diff_snapshots(
    previous: &HashMap<PathBuf, PathEntry>,
    current: &HashMap<PathBuf, PathEntry>,
) -> Vec<WatchEvent> {
    let mut events = Vec::new();
    for (path, entry) in current {
        match previous.get(path) {
            None => events.push(WatchEvent::Created(entry.clone())),
            Some(prev) => {
                if metadata_changed(prev, entry) {
                    events.push(WatchEvent::Modified(entry.clone()));
                }
            }
        }
    }
    for path in previous.keys() {
        if !current.contains_key(path) {
            events.push(WatchEvent::Removed(path.clone()));
        }
    }
    events
}

fn metadata_changed(a: &PathEntry, b: &PathEntry) -> bool {
    a.size() != b.size()
        || a.is_dir() != b.is_dir()
        || a.modified() != b.modified()
}

/// Expands filesystem globs (e.g. `*.rs`) into a stream of paths.
pub fn glob(pattern: impl AsRef<str>) -> Result<Shell<PathBuf>> {
    let mut matches = Vec::new();
    for entry in glob_iter(pattern.as_ref())? {
        matches.push(entry?);
    }
    Ok(Shell::from_iter(matches))
}

/// Expands globs while returning [`PathEntry`] metadata.
pub fn glob_entries(pattern: impl AsRef<str>) -> Result<Shell<PathEntry>> {
    let mut matches = Vec::new();
    for entry in glob_iter(pattern.as_ref())? {
        let path = entry?;
        let metadata = fs::metadata(&path)?;
        matches.push(PathEntry { path, metadata });
    }
    Ok(Shell::from_iter(matches))
}

/// Filters entries to only those matching the provided extension (case-insensitive).
pub fn filter_extension(
    entries: Shell<PathEntry>,
    ext: impl AsRef<str>,
) -> Shell<PathEntry> {
    let needle = ext.as_ref().to_ascii_lowercase();
    entries.filter(move |entry| {
        entry
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase() == needle)
            .unwrap_or(false)
    })
}

/// Keeps entries at or above the specified size (in bytes).
pub fn filter_size(
    entries: Shell<PathEntry>,
    min_bytes: u64,
) -> Shell<PathEntry> {
    entries.filter(move |entry| entry.size() >= min_bytes)
}

/// Keeps entries modified at or after `since`.
pub fn filter_modified_since(
    entries: Shell<PathEntry>,
    since: SystemTime,
) -> Shell<PathEntry> {
    entries.filter(move |entry| {
        entry
            .modified()
            .map(|time| time >= since)
            .unwrap_or(false)
    })
}

/// Creates a uniquely named temporary file and returns its path.
pub fn temp_file(prefix: impl AsRef<str>) -> Result<PathBuf> {
    let prefix = prefix.as_ref();
    let base = env::temp_dir();
    let pid = process::id();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    for attempt in 0..100 {
        let candidate =
            base.join(format!("{prefix}-{pid}-{now}-{attempt}.tmp"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "failed to allocate temporary file",
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn read_and_write_roundtrip() -> crate::Result<()> {
        let dir = tempdir()?;
        let file = dir.path().join("sample.txt");
        write_lines(&file, ["first", "second"])?;
        let lines = read_lines(&file)?.to_vec();
        assert_eq!(lines, vec!["first".to_string(), "second".to_string()]);
        Ok(())
    }

    #[test]
    fn glob_and_cat_helpers() -> crate::Result<()> {
        let dir = tempdir()?;
        let nested = dir.path().join("nested");
        mkdir_all(&nested)?;

        let file_a = dir.path().join("a.txt");
        let file_b = nested.join("b.txt");
        write_text(&file_a, "alpha\n")?;
        write_text(&file_b, "beta\n")?;
        append_text(&file_b, "beta-2\n")?;
        let orphan = dir.path().join("orphan.txt");
        write_text(&orphan, "single")?;

        let pattern = dir
            .path()
            .join("**")
            .join("*.txt")
            .to_string_lossy()
            .to_string();
        let mut matches = glob(&pattern)?.to_vec();
        matches.sort();
        assert!(matches.contains(&file_a));
        assert!(matches.contains(&file_b));
        assert!(matches.contains(&orphan));

        let cat_lines = cat([&file_a, &file_b])?.to_vec();
        assert_eq!(cat_lines.len(), 3);

        rm(&orphan)?;
        assert!(!orphan.exists());
        rm(&nested)?;
        assert!(!nested.exists());
        Ok(())
    }

    #[test]
    fn temp_and_detailed_listing() -> crate::Result<()> {
        let temp = temp_file("crab")?;
        append_text(&temp, "hello")?;
        assert!(temp.exists());
        rm(&temp)?;
        assert!(!temp.exists());

        let dir = tempdir()?;
        let file = dir.path().join("entry.txt");
        write_text(&file, "data")?;

        let detailed: Vec<_> = ls_detailed(dir.path())?.collect();
        assert!(detailed.iter().any(|entry| entry.path == file));

        let walk_entries: Vec<_> = walk_detailed(dir.path())?.collect();
        assert!(walk_entries.iter().any(|entry| entry.path == file));
        Ok(())
    }

    #[test]
    fn copy_move_and_walk_files() -> crate::Result<()> {
        let src = tempdir()?;
        let nested = src.path().join("nested");
        mkdir_all(&nested)?;
        let file = nested.join("data.txt");
        write_text(&file, "content")?;

        let dest = tempdir()?;
        let copy_target = dest.path().join("copy");
        copy_dir(src.path(), &copy_target)?;
        assert!(copy_target.join("nested").join("data.txt").exists());

        let move_target = dest.path().join("moved");
        move_path(&copy_target, &move_target)?;
        assert!(move_target.exists());
        assert!(!copy_target.exists());

        let files: Vec<_> = walk_files(&move_target)?.collect();
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].file_name().unwrap().to_string_lossy(),
            "data.txt"
        );

        let globbed: Vec<_> = glob_entries(
            move_target
                .join("**")
                .join("*.txt")
                .to_string_lossy()
                .to_string(),
        )?
        .collect();
        assert!(!globbed.is_empty());

        let filtered: Vec<_> =
            filter_extension(Shell::from_iter(globbed.clone()), "txt").collect();
        assert_eq!(filtered.len(), globbed.len());

        let filtered_size: Vec<_> =
            filter_size(Shell::from_iter(globbed.clone()), 1).collect();
        assert_eq!(filtered_size.len(), globbed.len());

        let filtered_recent: Vec<_> = filter_modified_since(
            Shell::from_iter(globbed.clone()),
            SystemTime::now() - Duration::from_secs(60),
        )
        .collect();
        assert!(!filtered_recent.is_empty());

        let dest_dir = tempdir()?;
        copy_entries(
            Shell::from_iter(globbed),
            move_target.parent().unwrap(),
            dest_dir.path(),
        )?;
        Ok(())
    }

    #[test]
    fn watcher_detects_changes() -> crate::Result<()> {
        let dir = tempdir()?;
        let file = dir.path().join("watched.txt");
        let mut watcher = Watcher::new(dir.path())?;
        assert!(watcher.poll()?.is_empty());

        write_text(&file, "one")?;
        let events = watcher.poll()?;
        assert!(matches!(
            events.as_slice(),
            [WatchEvent::Created(created)] if created.path == file
        ));

        write_text(&file, "two")?;
        let events = watcher.poll()?;
        assert!(!events.is_empty());

        rm(&file)?;
        let events = watcher.poll()?;
        assert!(matches!(
            events.as_slice(),
            [WatchEvent::Removed(path)] if path == &file
        ));
        Ok(())
    }
}

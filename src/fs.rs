use crate::{Error, Result, Shell};

use std::{
    collections::VecDeque,
    env,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process, thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use std::sync::mpsc::{self, Receiver};

#[cfg(feature = "async")]
use tokio::{sync::mpsc as async_mpsc, task};
#[cfg(feature = "async")]
use tokio_stream::wrappers::ReceiverStream;

use glob::{Pattern, glob as glob_iter};
use notify::Watcher as _;
use notify::{self, Event, EventKind, RecommendedWatcher, RecursiveMode, event::RemoveKind};

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
pub fn ls(path: impl AsRef<Path>) -> Result<Shell<Result<PathBuf>>> {
    let entries = fs::read_dir(path)?;
    Ok(Shell::new(Box::new(ReadDirPaths::new(entries))))
}

/// Lists the immediate children of a directory, including metadata.
pub fn ls_detailed(path: impl AsRef<Path>) -> Result<Shell<Result<PathEntry>>> {
    let entries = fs::read_dir(path)?;
    Ok(Shell::new(Box::new(ReadDirDetailed::new(entries))))
}

/// Recursively walks the directory tree depth-first including the root.
pub fn walk(root: impl AsRef<Path>) -> Result<Shell<Result<PathBuf>>> {
    Ok(Shell::new(Box::new(WalkIter::new(
        root.as_ref().to_path_buf(),
    ))))
}

/// Recursively walks the directory tree, including metadata for each entry.
pub fn walk_detailed(root: impl AsRef<Path>) -> Result<Shell<Result<PathEntry>>> {
    Ok(Shell::new(Box::new(WalkDetailedIter::new(
        root.as_ref().to_path_buf(),
    ))))
}

/// Walks the tree and yields only file entries.
pub fn walk_files(root: impl AsRef<Path>) -> Result<Shell<Result<PathEntry>>> {
    Ok(walk_detailed(root)?.filter_map(|entry| match entry {
        Ok(entry) if entry.is_file() => Some(Ok(entry)),
        Ok(_) => None,
        Err(err) => Some(Err(err)),
    }))
}

/// Walks the tree and keeps entries matching the predicate.
pub fn walk_filter<F>(root: impl AsRef<Path>, mut predicate: F) -> Result<Shell<Result<PathEntry>>>
where
    F: FnMut(&PathEntry) -> bool + 'static,
{
    Ok(walk_detailed(root)?.filter_map(move |entry| match entry {
        Ok(entry) => predicate(&entry).then_some(Ok(entry)),
        Err(err) => Some(Err(err)),
    }))
}

/// Reads a UTF-8 file completely into a `String`.
pub fn read_text(path: impl AsRef<Path>) -> Result<String> {
    Ok(fs::read_to_string(path)?)
}

/// Reads a file as a stream of lines.
pub fn read_lines(path: impl AsRef<Path>) -> Result<Shell<Result<String>>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    Ok(Shell::new(Box::new(
        reader.lines().map(|line| line.map_err(Into::into)),
    )))
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
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(contents.as_ref())?;
    Ok(())
}

/// Concatenates multiple files line-by-line.
pub fn cat<P, I>(paths: I) -> Result<Shell<Result<String>>>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = P>,
{
    let files = paths
        .into_iter()
        .map(|path| path.as_ref().to_path_buf())
        .collect::<Vec<_>>();
    Ok(Shell::new(Box::new(CatIter::new(files))))
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
    let walker = walk(from)?;
    for path in walker {
        let path = path?;
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
    entries: Shell<Result<PathEntry>>,
    root: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> Result<()> {
    let root = root.as_ref();
    let destination = destination.as_ref();
    for entry in entries {
        let entry = entry?;
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
    Removed { path: PathBuf, was_dir: bool },
}

impl WatchEvent {
    pub fn path(&self) -> &Path {
        match self {
            WatchEvent::Created(entry) | WatchEvent::Modified(entry) => &entry.path,
            WatchEvent::Removed { path, .. } => path,
        }
    }

    pub fn is_dir(&self) -> bool {
        match self {
            WatchEvent::Created(entry) | WatchEvent::Modified(entry) => entry.is_dir(),
            WatchEvent::Removed { was_dir, .. } => *was_dir,
        }
    }
}

/// Native watcher backed by the `notify` crate.
pub struct Watcher {
    _inner: RecommendedWatcher,
    rx: Receiver<std::result::Result<notify::Event, notify::Error>>,
}

impl Watcher {
    /// Starts watching `root` recursively for filesystem changes.
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;
        watcher.watch(&root, RecursiveMode::Recursive)?;
        Ok(Self {
            _inner: watcher,
            rx,
        })
    }

    /// Converts this watcher into a [`Shell`] that yields events as they occur.
    pub fn into_shell(self) -> Shell<Result<WatchEvent>> {
        Shell::new(WatcherIter::new(self._inner, self.rx))
    }

    /// Converts this watcher into a channel, allowing manual polling (`try_recv`).
    pub fn into_receiver(self) -> std::sync::mpsc::Receiver<Result<WatchEvent>> {
        let Watcher { _inner, rx } = self;
        let (tx, rx_out) = mpsc::channel();
        thread::spawn(move || {
            let _keep_alive = _inner;
            while let Ok(event) = rx.recv() {
                match event {
                    Ok(event) => {
                        let converted = convert_event(event);
                        if converted.is_empty() {
                            continue;
                        }
                        for item in converted {
                            if tx.send(Ok(item)).is_err() {
                                return;
                            }
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(Err(err.into()));
                        return;
                    }
                }
            }
        });
        rx_out
    }
}

struct WatcherIter {
    _inner: RecommendedWatcher,
    rx: Receiver<std::result::Result<notify::Event, notify::Error>>,
    pending: VecDeque<Result<WatchEvent>>,
}

impl WatcherIter {
    fn new(
        _inner: RecommendedWatcher,
        rx: Receiver<std::result::Result<notify::Event, notify::Error>>,
    ) -> Self {
        Self {
            _inner,
            rx,
            pending: VecDeque::new(),
        }
    }
}

impl Iterator for WatcherIter {
    type Item = Result<WatchEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some(event);
            }
            match self.rx.recv() {
                Ok(Ok(event)) => {
                    let converted = convert_event(event);
                    if converted.is_empty() {
                        continue;
                    }
                    self.pending.extend(converted.into_iter().map(Result::Ok));
                }
                Ok(Err(err)) => return Some(Err(err.into())),
                Err(_) => return None,
            }
        }
    }
}

/// Creates a lazy stream of filesystem changes under `root`.
pub fn watch(root: impl AsRef<Path>) -> Result<Shell<Result<WatchEvent>>> {
    Ok(Watcher::new(root)?.into_shell())
}

/// Returns a channel that yields filesystem events without blocking iteration.
pub fn watch_channel(
    root: impl AsRef<Path>,
) -> Result<std::sync::mpsc::Receiver<Result<WatchEvent>>> {
    Ok(Watcher::new(root)?.into_receiver())
}

/// Filters watch events by glob pattern (case-sensitive).
pub fn watch_glob(
    events: Shell<Result<WatchEvent>>,
    pattern: impl AsRef<str>,
) -> Result<Shell<Result<WatchEvent>>> {
    let pattern = Pattern::new(pattern.as_ref())?;
    Ok(events.filter(move |event| match event {
        Ok(event) => pattern.matches_path(event.path()),
        Err(_) => true,
    }))
}

/// Debounces watch events emitted by [`watch`], removing consecutive duplicates by path.
pub fn debounce_watch(
    events: Shell<Result<WatchEvent>>,
    window: Duration,
) -> Shell<Result<WatchEvent>> {
    let mut last_emitted: Option<(PathBuf, SystemTime)> = None;
    events.filter_map(move |event| match event {
        Ok(event) => {
            let (path, timestamp) = match &event {
                WatchEvent::Created(entry) | WatchEvent::Modified(entry) => (
                    entry.path.clone(),
                    entry.modified().unwrap_or_else(SystemTime::now),
                ),
                WatchEvent::Removed { path, .. } => (path.clone(), SystemTime::now()),
            };
            let should_emit = match &last_emitted {
                Some((last_path, last_time)) => {
                    last_path != &path
                        || timestamp.duration_since(*last_time).unwrap_or_default() >= window
                }
                None => true,
            };
            if should_emit {
                last_emitted = Some((path, timestamp));
                Some(Ok(event))
            } else {
                None
            }
        }
        Err(err) => Some(Err(err)),
    })
}

/// Convenience helper composing `watch`, `debounce_watch`, and `watch_glob`.
pub fn watch_filtered(
    root: impl AsRef<Path>,
    debounce_window: Duration,
    pattern: impl AsRef<str>,
) -> Result<Shell<Result<WatchEvent>>> {
    let events = watch(root)?;
    let debounced = debounce_watch(events, debounce_window);
    watch_glob(debounced, pattern)
}

/// Async watch helper that polls using `tokio::task::spawn_blocking`.
#[cfg(feature = "async")]
pub async fn watch_async(
    root: impl AsRef<Path> + Send + 'static,
    limit: usize,
) -> Result<Shell<Result<WatchEvent>>> {
    let root = root.as_ref().to_path_buf();
    let events = task::spawn_blocking(move || {
        let shell = watch(root)?;
        Ok::<Vec<_>, crate::Error>(shell.take(limit).collect())
    })
    .await
    .map_err(|err| {
        crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("watch task panicked: {err}"),
        ))
    })??;
    Ok(Shell::from_iter(events))
}

/// Async watch helper returning a `Stream` of change events.
#[cfg(feature = "async")]
pub async fn watch_async_stream(
    root: impl AsRef<Path> + Send + 'static,
) -> Result<ReceiverStream<Result<WatchEvent>>> {
    let root = root.as_ref().to_path_buf();
    let (tx, rx) = async_mpsc::channel(32);
    task::spawn_blocking(move || {
        let events = match watch(&root) {
            Ok(shell) => shell,
            Err(err) => {
                let _ = tx.blocking_send(Err(err));
                return;
            }
        };
        for event in events {
            if tx.blocking_send(event).is_err() {
                return;
            }
        }
    });
    Ok(ReceiverStream::new(rx))
}

/// Async convenience helper mirroring [`watch_filtered`].
#[cfg(feature = "async")]
pub async fn watch_filtered_async(
    root: impl AsRef<Path> + Send + 'static,
    limit: usize,
    debounce_window: Duration,
    pattern: impl AsRef<str>,
) -> Result<Shell<Result<WatchEvent>>> {
    let events = watch_async(root, limit).await?;
    let debounced = debounce_watch(events, debounce_window);
    watch_glob(debounced, pattern)
}

fn convert_event(event: Event) -> Vec<WatchEvent> {
    let mut out = Vec::new();
    for path in event.paths {
        match &event.kind {
            EventKind::Create(_) => {
                if let Some(entry) = path_entry_for(&path) {
                    out.push(WatchEvent::Created(entry));
                }
            }
            EventKind::Modify(_) | EventKind::Any | EventKind::Other => {
                if let Some(entry) = path_entry_for(&path) {
                    out.push(WatchEvent::Modified(entry));
                }
            }
            EventKind::Remove(kind) => {
                let was_dir = matches!(kind, RemoveKind::Folder | RemoveKind::Any);
                out.push(WatchEvent::Removed { path, was_dir });
            }
            _ => {}
        }
    }
    out
}

fn path_entry_for(path: &Path) -> Option<PathEntry> {
    fs::metadata(path).ok().map(|metadata| PathEntry {
        path: path.to_path_buf(),
        metadata,
    })
}

struct ReadDirPaths {
    inner: fs::ReadDir,
}

impl ReadDirPaths {
    fn new(inner: fs::ReadDir) -> Self {
        Self { inner }
    }
}

impl Iterator for ReadDirPaths {
    type Item = Result<PathBuf>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|entry| entry.map(|entry| entry.path()).map_err(Into::into))
    }
}

struct ReadDirDetailed {
    inner: fs::ReadDir,
}

impl ReadDirDetailed {
    fn new(inner: fs::ReadDir) -> Self {
        Self { inner }
    }
}

impl Iterator for ReadDirDetailed {
    type Item = Result<PathEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|entry| {
            let entry = entry?;
            let metadata = entry.metadata()?;
            Ok(PathEntry {
                path: entry.path(),
                metadata,
            })
        })
    }
}

struct WalkIter {
    stack: Vec<PathBuf>,
    pending_err: Option<Error>,
}

impl WalkIter {
    fn new(root: PathBuf) -> Self {
        Self {
            stack: vec![root],
            pending_err: None,
        }
    }
}

impl Iterator for WalkIter {
    type Item = Result<PathBuf>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(err) = self.pending_err.take() {
            return Some(Err(err));
        }
        let path = self.stack.pop()?;
        if path.is_dir() {
            match fs::read_dir(&path) {
                Ok(read_dir) => {
                    for entry in read_dir {
                        match entry {
                            Ok(entry) => self.stack.push(entry.path()),
                            Err(err) => {
                                self.pending_err = Some(err.into());
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    self.pending_err = Some(err.into());
                }
            }
        }
        Some(Ok(path))
    }
}

struct WalkDetailedIter {
    stack: Vec<PathBuf>,
    pending_err: Option<Error>,
}

impl WalkDetailedIter {
    fn new(root: PathBuf) -> Self {
        Self {
            stack: vec![root],
            pending_err: None,
        }
    }
}

impl Iterator for WalkDetailedIter {
    type Item = Result<PathEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(err) = self.pending_err.take() {
            return Some(Err(err));
        }
        let path = self.stack.pop()?;
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) => return Some(Err(err.into())),
        };
        if metadata.is_dir() {
            match fs::read_dir(&path) {
                Ok(read_dir) => {
                    for entry in read_dir {
                        match entry {
                            Ok(entry) => self.stack.push(entry.path()),
                            Err(err) => {
                                self.pending_err = Some(err.into());
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    self.pending_err = Some(err.into());
                }
            }
        }
        Some(Ok(PathEntry { path, metadata }))
    }
}

struct CatIter {
    files: Vec<PathBuf>,
    idx: usize,
    current: Option<io::Lines<BufReader<File>>>,
}

impl CatIter {
    fn new(files: Vec<PathBuf>) -> Self {
        Self {
            files,
            idx: 0,
            current: None,
        }
    }

    fn advance_reader(&mut self) -> Option<Result<()>> {
        if self.idx >= self.files.len() {
            return None;
        }
        let path = &self.files[self.idx];
        self.idx += 1;
        match File::open(path) {
            Ok(file) => {
                self.current = Some(BufReader::new(file).lines());
                Some(Ok(()))
            }
            Err(err) => Some(Err(err.into())),
        }
    }
}

impl Iterator for CatIter {
    type Item = Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(lines) = &mut self.current {
                match lines.next() {
                    Some(Ok(line)) => return Some(Ok(line)),
                    Some(Err(err)) => return Some(Err(err.into())),
                    None => {
                        self.current = None;
                        continue;
                    }
                }
            } else if let Some(result) = self.advance_reader() {
                match result {
                    Ok(()) => continue,
                    Err(err) => return Some(Err(err)),
                }
            } else {
                return None;
            }
        }
    }
}

/// Expands filesystem globs (e.g. `*.rs`) into a stream of paths.
pub fn glob(pattern: impl AsRef<str>) -> Result<Shell<Result<PathBuf>>> {
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
        let candidate = base.join(format!("{prefix}-{pid}-{now}-{attempt}.tmp"));
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
        let lines = read_lines(&file)?.collect::<crate::Result<Vec<_>>>()?;
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
        let mut matches = glob(&pattern)?.collect::<crate::Result<Vec<_>>>()?;
        matches.sort();
        assert!(matches.contains(&file_a));
        assert!(matches.contains(&file_b));
        assert!(matches.contains(&orphan));

        let cat_lines = cat([&file_a, &file_b])?.collect::<crate::Result<Vec<_>>>()?;
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

        let detailed: Vec<_> = ls_detailed(dir.path())?.collect::<crate::Result<Vec<_>>>()?;
        assert!(detailed.iter().any(|entry| entry.path == file));

        let walk_entries: Vec<_> = walk_detailed(dir.path())?.collect::<crate::Result<Vec<_>>>()?;
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

        let files: Vec<_> = walk_files(&move_target)?.collect::<crate::Result<Vec<_>>>()?;
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap().to_string_lossy(), "data.txt");

        let globbed: Vec<_> = glob_entries(
            move_target
                .join("**")
                .join("*.txt")
                .to_string_lossy()
                .to_string(),
        )?
        .collect::<crate::Result<Vec<_>>>()?;
        assert!(!globbed.is_empty());

        let filtered: Vec<_> =
            filter_extension(Shell::from_iter(globbed.clone().into_iter().map(Ok)), "txt")
                .collect::<crate::Result<Vec<_>>>()?;
        assert_eq!(filtered.len(), globbed.len());

        let filtered_size: Vec<_> =
            filter_size(Shell::from_iter(globbed.clone().into_iter().map(Ok)), 1)
                .collect::<crate::Result<Vec<_>>>()?;
        assert_eq!(filtered_size.len(), globbed.len());

        let filtered_recent: Vec<_> = filter_modified_since(
            Shell::from_iter(globbed.clone().into_iter().map(Ok)),
            SystemTime::now() - Duration::from_secs(60),
        )
        .collect::<crate::Result<Vec<_>>>()?;
        assert!(!filtered_recent.is_empty());

        let dest_dir = tempdir()?;
        copy_entries(
            Shell::from_iter(globbed.into_iter().map(Ok)),
            move_target.parent().unwrap(),
            dest_dir.path(),
        )?;
        Ok(())
    }

    #[test]
    fn watcher_detects_changes() -> crate::Result<()> {
        let dir = tempdir()?;
        let file = dir.path().join("watched.txt");
        let mut events = watch(dir.path())?;

        write_text(&file, "one")?;
        let created_path = file.clone();
        let created = next_event(&mut events, move |event| match event {
            WatchEvent::Created(entry) => entry.path == created_path,
            _ => false,
        })?;
        assert!(matches!(created, WatchEvent::Created(entry) if entry.path == file));

        write_text(&file, "two")?;
        // Drain whichever event is next for coverage.
        let _ = next_event(&mut events, |_| true)?;

        rm(&file)?;
        let removed_path = file.clone();
        let removed = next_event(&mut events, move |event| match event {
            WatchEvent::Removed { path, .. } => path == &removed_path,
            _ => false,
        })?;
        assert!(matches!(removed, WatchEvent::Removed { path, .. } if path == file));
        Ok(())
    }

    #[test]
    fn watch_channel_receives_events() -> crate::Result<()> {
        let dir = tempdir()?;
        let file = dir.path().join("chan.txt");
        let rx = watch_channel(dir.path())?;
        write_text(&file, "one")?;
        let event = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("watch channel timed out")?;
        assert_eq!(event.path(), file.as_path());
        Ok(())
    }

    fn next_event<F>(
        events: &mut Shell<Result<WatchEvent>>,
        predicate: F,
    ) -> crate::Result<WatchEvent>
    where
        F: Fn(&WatchEvent) -> bool,
    {
        loop {
            let event = events.next().expect("watch stream closed")?;
            if predicate(&event) {
                return Ok(event);
            }
        }
    }
}

use crate::{Result, Shell};

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime},
};

use std::sync::mpsc::{self, Receiver};

#[cfg(feature = "async")]
use tokio::{sync::mpsc as async_mpsc, task};
#[cfg(feature = "async")]
use tokio_stream::wrappers::ReceiverStream;

use notify::Watcher as _;
use notify::{
    self, Event, EventKind, RecommendedWatcher, RecursiveMode,
    event::{ModifyKind, RemoveKind, RenameMode},
};

use super::{
    entries::{PathEntry, path_entry_for},
    glob::watch_glob,
};

/// File system change events emitted by [`Watcher`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    Created(PathEntry),
    Modified(PathEntry),
    Removed {
        path: PathBuf,
        was_dir: bool,
    },
    Renamed {
        from: PathBuf,
        to: PathBuf,
        entry: Option<PathEntry>,
    },
}

impl WatchEvent {
    pub fn path(&self) -> &Path {
        match self {
            WatchEvent::Created(entry) | WatchEvent::Modified(entry) => &entry.path,
            WatchEvent::Removed { path, .. } => path,
            WatchEvent::Renamed { to, entry, .. } => entry
                .as_ref()
                .map(|entry| entry.path.as_path())
                .unwrap_or(to),
        }
    }

    pub fn is_dir(&self) -> bool {
        match self {
            WatchEvent::Created(entry) | WatchEvent::Modified(entry) => entry.is_dir(),
            WatchEvent::Removed { was_dir, .. } => *was_dir,
            WatchEvent::Renamed { entry, .. } => {
                entry.as_ref().map(PathEntry::is_dir).unwrap_or(false)
            }
        }
    }

    /// Returns the source path for rename events, if available.
    pub fn from_path(&self) -> Option<&Path> {
        match self {
            WatchEvent::Renamed { from, .. } => Some(from.as_path()),
            _ => None,
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
                WatchEvent::Renamed { to, entry, .. } => (
                    entry
                        .as_ref()
                        .map(|entry| entry.path.clone())
                        .unwrap_or_else(|| to.clone()),
                    entry
                        .as_ref()
                        .and_then(|entry| entry.modified())
                        .unwrap_or_else(SystemTime::now),
                ),
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
        crate::Error::Io(std::io::Error::other(format!("watch task panicked: {err}")))
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
    match event.kind {
        EventKind::Modify(ModifyKind::Name(mode)) => convert_rename_event(mode, event.paths),
        kind => convert_standard_event(kind, event.paths),
    }
}

fn convert_standard_event(kind: EventKind, paths: Vec<PathBuf>) -> Vec<WatchEvent> {
    let mut out = Vec::new();
    for path in paths {
        match &kind {
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
                let was_dir = matches!(kind, RemoveKind::Folder | RemoveKind::Any) || path.is_dir();
                out.push(WatchEvent::Removed { path, was_dir });
            }
            _ => {}
        }
    }
    out
}

fn convert_rename_event(mode: RenameMode, paths: Vec<PathBuf>) -> Vec<WatchEvent> {
    match mode {
        RenameMode::Both | RenameMode::Any => {
            if paths.len() >= 2 {
                let mut iter = paths.into_iter();
                let from = iter.next().unwrap();
                let to = iter.next().unwrap();
                let entry = path_entry_for(&to);
                vec![WatchEvent::Renamed { from, to, entry }]
            } else {
                convert_as_modified(paths)
            }
        }
        RenameMode::To => paths
            .into_iter()
            .filter_map(|path| path_entry_for(&path).map(WatchEvent::Created))
            .collect(),
        RenameMode::From => paths
            .into_iter()
            .map(|path| WatchEvent::Removed {
                was_dir: path_entry_for(&path)
                    .map(|entry| entry.is_dir())
                    .unwrap_or(false),
                path,
            })
            .collect(),
        RenameMode::Other => convert_as_modified(paths),
    }
}

fn convert_as_modified(paths: Vec<PathBuf>) -> Vec<WatchEvent> {
    paths
        .into_iter()
        .filter_map(|path| path_entry_for(&path).map(WatchEvent::Modified))
        .collect()
}

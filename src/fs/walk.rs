use crate::{Error, Result, Shell};

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::entries::PathEntry;

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

/// Walks the tree and yields only file entries (follows symlinks to files).
pub fn walk_files(root: impl AsRef<Path>) -> Result<Shell<Result<PathEntry>>> {
    Ok(walk_detailed(root)?.filter_map(|entry| match entry {
        Ok(entry) if is_file_or_symlink_to_file(&entry) => Some(Ok(entry)),
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

fn is_file_or_symlink_to_file(entry: &PathEntry) -> bool {
    if entry.is_file() {
        return true;
    }
    if entry.metadata.file_type().is_symlink() {
        return fs::metadata(&entry.path)
            .map(|m| m.is_file())
            .unwrap_or(false);
    }
    false
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

    fn push_children(&mut self, dir: &Path) {
        match fs::read_dir(dir) {
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
}

impl Iterator for WalkIter {
    type Item = Result<PathBuf>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(err) = self.pending_err.take() {
            return Some(Err(err));
        }
        let path = self.stack.pop()?;
        let should_descend = match fs::symlink_metadata(&path) {
            Ok(meta) => meta.file_type().is_dir() && !meta.file_type().is_symlink(),
            Err(err) => {
                self.pending_err = Some(err.into());
                false
            }
        };
        if should_descend {
            self.push_children(&path);
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
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) => return Some(Err(err.into())),
        };
        let should_descend = match fs::symlink_metadata(&path) {
            Ok(meta) => meta.file_type().is_dir() && !meta.file_type().is_symlink(),
            Err(err) => {
                self.pending_err = Some(err.into());
                false
            }
        };
        if should_descend {
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

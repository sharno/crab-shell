use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

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

pub(crate) fn path_entry_for(path: &Path) -> Option<PathEntry> {
    fs::symlink_metadata(path).ok().map(|metadata| PathEntry {
        path: path.to_path_buf(),
        metadata,
    })
}

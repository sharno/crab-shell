use crate::{Result, Shell};

use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use super::entries::PathEntry;

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
    let metadata = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    let file_type = metadata.file_type();
    if file_type.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Recursively copies a directory tree.
pub fn copy_dir(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    mkdir_all(to)?;
    let walker = super::walk::walk(from)?;
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

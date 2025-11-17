use crate::{Result, Shell};

use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use glob::glob as glob_iter;

/// Metadata about a filesystem path captured during listing operations.
#[derive(Debug)]
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
}

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

/// Expands filesystem globs (e.g. `*.rs`) into a stream of paths.
pub fn glob(pattern: impl AsRef<str>) -> Result<Shell<PathBuf>> {
    let mut matches = Vec::new();
    for entry in glob_iter(pattern.as_ref())? {
        matches.push(entry?);
    }
    Ok(Shell::from_iter(matches))
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
}

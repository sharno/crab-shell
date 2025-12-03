use std::{
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

/// Returns the value of an environment variable.
pub fn var(key: impl AsRef<OsStr>) -> Option<OsString> {
    env::var_os(key)
}

/// Sets an environment variable for the current process.
///
/// Mirrors [`std::env::set_var`] but keeps the crate API surface cohesive.
/// Panics if `key` is empty or contains an equals sign, just like the standard
/// library call.
///
/// # Examples
///
/// ```
/// use qshr::prelude::*;
///
/// set_var("QSHR_EXAMPLE", "value");
/// assert_eq!(var("QSHR_EXAMPLE").unwrap(), "value");
/// remove_var("QSHR_EXAMPLE");
/// ```
pub fn set_var(key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) {
    unsafe {
        env::set_var(key, value);
    }
}

/// Removes an environment variable for the current process.
///
/// This is a thin wrapper around [`std::env::remove_var`]; removing a missing
/// entry is a no-op.
pub fn remove_var(key: impl AsRef<OsStr>) {
    unsafe {
        env::remove_var(key);
    }
}

/// Returns the user's home directory, if any.
pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Returns the PATH entries as a vector.
pub fn path_entries() -> Vec<PathBuf> {
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).collect())
        .unwrap_or_default()
}

/// Finds a program on PATH, similar to the `which` command.
pub fn which(program: impl AsRef<OsStr>) -> Option<PathBuf> {
    let program = program.as_ref();
    let path = Path::new(program);
    // If the user provided an explicit path (absolute or relative), resolve it directly.
    if path.is_absolute() || path.components().count() > 1 {
        let meta = std::fs::symlink_metadata(path).ok()?;
        if meta.file_type().is_dir() {
            return None;
        }
        if let Ok(canon) = path.canonicalize() {
            return canon.is_file().then_some(canon);
        }
        return meta.is_file().then_some(path.to_path_buf());
    }
    #[cfg(windows)]
    let pathext = pathext_extensions();
    #[cfg(windows)]
    let has_ext = Path::new(program).extension().is_some();
    for dir in path_entries() {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            if has_ext {
                continue;
            }
            for ext in &pathext {
                let candidate = candidate.with_extension(ext);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

#[cfg(windows)]
fn pathext_extensions() -> Vec<String> {
    env::var_os("PATHEXT")
        .map(|val| {
            val.to_string_lossy()
                .split(';')
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_start_matches('.').to_ascii_lowercase())
                .collect()
        })
        .filter(|exts: &Vec<String>| !exts.is_empty())
        .unwrap_or_else(|| {
            vec!["com", "exe", "bat", "cmd"]
                .into_iter()
                .map(str::to_string)
                .collect()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get_env() {
        set_var("CRAB_SHELL_TEST_VAR", "abc");
        assert_eq!(
            var("CRAB_SHELL_TEST_VAR").and_then(|v| v.into_string().ok()),
            Some("abc".into())
        );
        remove_var("CRAB_SHELL_TEST_VAR");
        assert!(var("CRAB_SHELL_TEST_VAR").is_none());
    }

    #[test]
    fn removing_missing_var_is_safe() {
        remove_var("CRAB_SHELL_MISSING_VAR");
        assert!(var("CRAB_SHELL_MISSING_VAR").is_none());
    }

    #[test]
    fn which_resolves_relative_paths() {
        let cwd = std::env::current_dir().unwrap();
        let dir = tempfile::tempdir_in(&cwd).unwrap();
        let nested = dir.path().join("bin");
        std::fs::create_dir_all(&nested).unwrap();
        let target = nested.join("script.sh");
        std::fs::write(&target, b"echo hi").unwrap();

        let relative = target.strip_prefix(&cwd).unwrap();
        let result = which(relative).unwrap();
        assert_eq!(
            result.canonicalize().unwrap(),
            target.canonicalize().unwrap()
        );
    }

    #[test]
    fn which_ignores_directories() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("bin");
        std::fs::create_dir_all(&subdir).unwrap();
        assert!(which(&subdir).is_none());
    }
}

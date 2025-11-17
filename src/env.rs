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
pub fn set_var(key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) {
    unsafe {
        env::set_var(key, value);
    }
}

/// Removes an environment variable for the current process.
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
    if Path::new(program).is_absolute() {
        return PathBuf::from(program).canonicalize().ok();
    }
    for dir in path_entries() {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            const EXTENSIONS: [&str; 3] = ["exe", "cmd", "bat"];
            for ext in EXTENSIONS {
                let candidate = dir.join(format!("{}.{}", program.to_string_lossy(), ext));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
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
}

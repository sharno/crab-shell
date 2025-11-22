use super::*;
use crate::Shell;
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

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = move_target.join("data-link");
        symlink(&files[0].path, &link)?;
        let names: Vec<_> = walk_files(&move_target)?
            .collect::<crate::Result<Vec<_>>>()?
            .into_iter()
            .map(|e| e.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|n| n == "data-link"));

        let dir_link = move_target.join("dir-link");
        symlink(move_target.join("nested"), &dir_link)?;
        let names: Vec<_> = walk_files(&move_target)?
            .collect::<crate::Result<Vec<_>>>()?
            .into_iter()
            .map(|e| e.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(
            !names.iter().any(|n| n == "dir-link"),
            "directory symlink should be excluded from walk_files"
        );
    }

    let glob_pattern = move_target.join("**").join("*.txt");
    let globbed: Vec<_> =
        glob_entries(glob_pattern.to_string_lossy())?.collect::<crate::Result<Vec<_>>>()?;
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
        std::time::SystemTime::now() - Duration::from_secs(60),
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

#[cfg(unix)]
#[test]
fn rm_removes_symlink_without_descending() -> crate::Result<()> {
    use std::os::unix::fs as unix_fs;

    let dir = tempdir()?;
    let target = dir.path().join("target");
    mkdir_all(&target)?;
    let nested = target.join("file.txt");
    write_text(&nested, "keep me")?;

    let link = dir.path().join("link");
    unix_fs::symlink(&target, &link)?;

    rm(&link)?;
    assert!(!link.exists(), "symlink should be removed");
    assert!(target.exists(), "target directory should remain");
    assert!(nested.exists(), "nested file should remain");
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
fn watcher_reports_renames() -> crate::Result<()> {
    let dir = tempdir()?;
    let from = dir.path().join("from.txt");
    let to = dir.path().join("to.txt");
    let mut events = watch(dir.path())?;

    write_text(&from, "seed")?;
    let _ = next_event(&mut events, |_| true)?;

    std::fs::rename(&from, &to)?;
    let renamed = next_event(&mut events, |event| {
        matches!(event, WatchEvent::Renamed { .. })
    })?;
    match &renamed {
        WatchEvent::Renamed {
            from: old, to: new, ..
        } => {
            assert_eq!(old, &from);
            assert_eq!(new, &to);
        }
        _ => unreachable!(),
    }
    assert_eq!(renamed.path(), to.as_path());
    assert_eq!(renamed.from_path(), Some(from.as_path()));
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

#[cfg(unix)]
#[test]
fn walk_avoids_symlink_cycles() -> crate::Result<()> {
    use std::collections::HashSet;
    use std::os::unix::fs::symlink;

    let dir = tempdir()?;
    let root = dir.path().join("root");
    mkdir_all(&root)?;
    let file = root.join("leaf.txt");
    write_text(&file, "hello")?;

    let link = root.join("loop");
    symlink(&root, &link)?;

    let entries: Vec<_> = walk_detailed(&root)?
        .take(10)
        .collect::<crate::Result<Vec<_>>>()?;
    let unique: HashSet<_> = entries.iter().map(|e| e.path.clone()).collect();
    assert_eq!(entries.len(), unique.len(), "walk produced duplicate paths");
    assert!(entries.iter().any(|e| e.path == link));
    assert!(entries.iter().any(|e| e.path == file));
    Ok(())
}

fn next_event<F>(events: &mut Shell<crate::Result<WatchEvent>>, predicate: F) -> crate::Result<WatchEvent>
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

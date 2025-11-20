use std::time::{Duration, SystemTime};

use qshr::prelude::*;
use tempfile::tempdir;

#[test]
fn filesystem_helpers_cover_common_paths() -> qshr::Result<()> {
    let temp = tempdir()?;
    let root = temp.path();
    let work = root.join("work");
    let nested = work.join("nested");
    mkdir_all(&nested)?;

    let file_a = work.join("a.txt");
    let file_b = nested.join("b.txt");
    write_text(&file_a, "alpha\n")?;
    write_lines(&file_b, ["bravo", "charlie"])?;
    append_text(&file_b, "delta\n")?;

    let lines = read_lines(&file_b)?.collect::<qshr::Result<Vec<_>>>()?;
    assert!(lines.contains(&"delta".to_string()));

    let cat_lines = cat([&file_a, &file_b])?.collect::<qshr::Result<Vec<_>>>()?;
    assert!(cat_lines.len() >= 4);

    let glob_pattern = work.join("**").join("*.txt").to_string_lossy().to_string();
    let mut globbed = glob(&glob_pattern)?.collect::<qshr::Result<Vec<_>>>()?;
    globbed.sort();
    assert!(globbed.contains(&file_a));
    assert!(globbed.contains(&file_b));

    let detailed = glob_entries(&glob_pattern)?.collect::<qshr::Result<Vec<_>>>()?;
    assert!(
        detailed
            .iter()
            .all(|entry| entry.path.extension().unwrap() == "txt")
    );

    let filtered =
        filter_extension(glob_entries(&glob_pattern)?, "txt").collect::<qshr::Result<Vec<_>>>()?;
    assert!(filtered.len() >= 2);

    let min_size =
        filter_size(glob_entries(&glob_pattern)?, 4).collect::<qshr::Result<Vec<_>>>()?;
    assert!(!min_size.is_empty());

    let since = SystemTime::now() - Duration::from_secs(60);
    let recent = filter_modified_since(glob_entries(&glob_pattern)?, since)
        .collect::<qshr::Result<Vec<_>>>()?;
    assert!(recent.len() >= 2);

    let ls_entries = ls(&work)?.collect::<qshr::Result<Vec<_>>>()?;
    assert!(ls_entries.iter().any(|path| path == &nested));
    let ls_detailed_entries = ls_detailed(&work)?.collect::<qshr::Result<Vec<_>>>()?;
    assert_eq!(ls_detailed_entries.len(), ls_entries.len());

    let walked = walk(&work)?.collect::<qshr::Result<Vec<_>>>()?;
    assert!(walked.contains(&nested));
    let files_only = walk_files(&work)?.collect::<qshr::Result<Vec<_>>>()?;
    assert!(files_only.iter().all(|entry| entry.is_file()));

    let nested_clone = nested.clone();
    let only_nested = walk_filter(&work, move |entry| entry.path.starts_with(&nested_clone))?
        .collect::<qshr::Result<Vec<_>>>()?;
    assert!(
        only_nested
            .iter()
            .all(|entry| entry.path.starts_with(&nested))
    );

    let copy_target = root.join("copy");
    copy_dir(&work, &copy_target)?;
    assert!(copy_target.join("a.txt").exists());

    let move_target = temp.path().join("moved");
    move_path(&copy_target, &move_target)?;
    assert!(move_target.join("a.txt").exists());

    let entries = walk_detailed(&work)?;
    let mirror = root.join("mirror");
    copy_entries(entries, &work, &mirror)?;
    assert!(mirror.join("nested/b.txt").exists());

    rm(&mirror)?;
    assert!(!mirror.exists());

    let tmp = temp_file("qshr-fs-test")?;
    append_text(&tmp, "hello")?;
    assert!(tmp.exists());
    rm(&tmp)?;
    assert!(!tmp.exists());

    Ok(())
}

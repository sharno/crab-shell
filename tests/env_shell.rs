use qshr::prelude::*;

#[test]
fn env_helpers_roundtrip() {
    set_var("QSHR_ENV_TEST", "value");
    assert_eq!(var("QSHR_ENV_TEST").unwrap().to_str(), Some("value"));
    remove_var("QSHR_ENV_TEST");
    assert!(var("QSHR_ENV_TEST").is_none());

    assert!(home_dir().is_some());
    assert!(!path_entries().is_empty());
    assert!(which("sh").is_some());
}

#[test]
fn shell_combinators_cover_common_paths() {
    let collected: Vec<_> = Shell::from_iter(0..6)
        .map(|n| n + 1)
        .filter(|n| n % 2 == 0)
        .enumerate()
        .map(|(idx, val)| idx + val)
        .take(3)
        .collect();
    assert_eq!(collected, vec![2, 5, 8]);

    let sums: Vec<i32> = Shell::from_iter(0..9)
        .chunk_map(3, |chunk| vec![chunk.into_iter().sum()])
        .collect();
    assert_eq!(sums, vec![3, 12, 21]);

    let chunk_sizes = Shell::from_iter(0..5)
        .chunks(2)
        .map(|chunk| chunk.len())
        .collect::<Vec<_>>();
    assert_eq!(chunk_sizes, vec![2, 2, 1]);

    let windows = Shell::from_iter(1..5)
        .windows(2)
        .map(|pair| pair.iter().sum::<i32>())
        .collect::<Vec<_>>();
    assert_eq!(windows, vec![3, 5, 7]);

    let joined = Shell::from_iter(["c", "b", "a"])
        .sorted()
        .distinct()
        .join(",");
    assert_eq!(joined, "a,b,c");

    let mut double_ended = DoubleEndedShell::from_vec(vec![1, 2, 3, 4]);
    assert_eq!(double_ended.next(), Some(1));
    assert_eq!(double_ended.next_back(), Some(4));

    let product = Shell::from_iter(1..3).product(4..6).collect::<Vec<_>>();
    assert_eq!(product, vec![(1, 4), (1, 5), (2, 4), (2, 5)]);

    let zipped = Shell::from_iter(0..3).zip(3..6).collect::<Vec<_>>();
    assert_eq!(zipped, vec![(0, 3), (1, 4), (2, 5)]);

    let folded = Shell::from_iter(1..4).fold(0, |acc, x| acc + x);
    assert_eq!(folded, 6);
}

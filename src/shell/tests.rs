use super::{DoubleEndedShell, Shell};

#[test]
fn len_hint_tracks_iterator() {
    let mut shell = Shell::from_iter([1, 2, 3]);
    assert_eq!(shell.len_hint(), (3, Some(3)));
    assert_eq!(shell.next(), Some(1));
    assert_eq!(shell.len_hint(), (2, Some(2)));
}

#[test]
fn filter_map_chain() {
    let values: Vec<_> = Shell::from_iter(0..6)
        .filter_map(|n| (n % 2 == 0).then_some(n * 10))
        .take(2)
        .collect();
    assert_eq!(values, vec![0, 20]);
}

#[test]
fn join_and_fold() {
    let joined = Shell::from_iter(["a", "b", "c"]).join(",");
    assert_eq!(joined, "a,b,c");
    let sum = Shell::from_iter([1, 2, 3]).fold(0, |acc, n| acc + n);
    assert_eq!(sum, 6);
}

#[test]
fn chunk_and_zip() {
    let chunked: Vec<Vec<_>> = Shell::from_iter(1..=5).chunks(2).collect();
    assert_eq!(chunked, vec![vec![1, 2], vec![3, 4], vec![5]]);

    let zipped: Vec<_> = Shell::from_iter(["a".to_string(), "b".to_string()])
        .zip(["x", "y"])
        .collect();
    assert_eq!(
        zipped,
        vec![("a".to_string(), "x"), ("b".to_string(), "y"),]
    );
}

#[test]
fn windows_interleave_product() {
    let windows: Vec<_> = Shell::from_iter([1, 2, 3, 4]).windows(3).collect();
    assert_eq!(windows, vec![vec![1, 2, 3], vec![2, 3, 4]]);

    let interleaved: Vec<_> = Shell::from_iter([1, 3, 5]).interleave([2, 4, 6]).collect();
    assert_eq!(interleaved, vec![1, 2, 3, 4, 5, 6]);

    let product: Vec<_> = Shell::from_iter(["a", "b"]).product(["x", "y"]).collect();
    assert_eq!(
        product,
        vec![("a", "x"), ("a", "y"), ("b", "x"), ("b", "y"),]
    );
}

#[test]
fn distinct_and_sorted() {
    let distinct: Vec<_> = Shell::from_iter([1, 2, 2, 3, 1]).distinct().collect();
    assert_eq!(distinct, vec![1, 2, 3]);

    let sorted: Vec<_> = Shell::from_iter([3, 1, 2]).sorted().collect();
    assert_eq!(sorted, vec![1, 2, 3]);
}

#[test]
fn chunk_map_transforms() {
    let values: Vec<_> = Shell::from_iter(0..6)
        .chunk_map(2, |chunk| chunk.into_iter().map(|n| n * 2).collect())
        .collect();
    assert_eq!(values, vec![0, 2, 4, 6, 8, 10]);
}

#[cfg(feature = "parallel")]
#[test]
fn chunk_map_parallel_transforms() {
    let values: Vec<_> = Shell::from_iter(0..6)
        .chunk_map_parallel(2, |chunk| chunk.into_iter().map(|n| n * 2).collect())
        .collect();
    assert_eq!(values, vec![0, 2, 4, 6, 8, 10]);
}

#[test]
fn double_ended_shell_pops_back() {
    let mut shell = DoubleEndedShell::from_vec(vec![1, 2, 3]);
    assert_eq!(shell.next(), Some(1));
    assert_eq!(shell.next_back(), Some(3));
    assert_eq!(shell.into_shell().to_vec(), vec![2]);
}

use std::{
    collections::{HashSet, VecDeque},
    iter,
    sync::Arc,
    vec::IntoIter,
};

/// A lazy, composable stream of values inspired by Turtle's `Shell`.
///
/// Internally `Shell` is a boxed iterator which keeps the type signature
/// stable when chaining multiple transformations together.
pub struct Shell<T> {
    iter: Box<dyn Iterator<Item = T> + 'static>,
}

impl<T> Shell<T> {
    /// Wraps an arbitrary iterator.
    pub fn new<I>(iter: I) -> Self
    where
        I: Iterator<Item = T> + 'static,
    {
        Self {
            iter: Box::new(iter),
        }
    }

    /// An empty stream.
    pub fn empty() -> Self
    where
        T: 'static,
    {
        Self::new(iter::empty())
    }

    /// A single value stream.
    pub fn one(item: T) -> Self
    where
        T: 'static,
    {
        Self::new(iter::once(item))
    }

    /// A stream driven by a closure.
    pub fn from_fn<F>(f: F) -> Self
    where
        F: FnMut() -> Option<T> + 'static,
        T: 'static,
    {
        Self::new(iter::from_fn(f))
    }

    /// Applies a transformation.
    pub fn map<U, F>(self, f: F) -> Shell<U>
    where
        U: 'static,
        F: FnMut(T) -> U + 'static,
        T: 'static,
    {
        let iter = self.into_boxed();
        Shell::new(iter.map(f))
    }

    /// Filters elements by a predicate.
    pub fn filter<F>(self, predicate: F) -> Shell<T>
    where
        F: FnMut(&T) -> bool + 'static,
        T: 'static,
    {
        let iter = self.into_boxed();
        Shell::new(iter.filter(predicate))
    }

    /// Applies a filter-map transformation.
    pub fn filter_map<U, F>(self, f: F) -> Shell<U>
    where
        F: FnMut(T) -> Option<U> + 'static,
        T: 'static,
        U: 'static,
    {
        let iter = self.into_boxed();
        Shell::new(iter.filter_map(f))
    }

    /// Flat maps each value to another iterable.
    pub fn then<U, F, I>(self, f: F) -> Shell<U>
    where
        U: 'static,
        F: FnMut(T) -> I + 'static,
        I: IntoIterator<Item = U> + 'static,
        I::IntoIter: Iterator<Item = U> + 'static,
        T: 'static,
    {
        let iter = self.into_boxed();
        Shell::new(iter.flat_map(f))
    }

    /// Yields at most `n` elements.
    pub fn take(self, n: usize) -> Shell<T>
    where
        T: 'static,
    {
        let iter = self.into_boxed();
        Shell::new(iter.take(n))
    }

    /// Yields elements while the predicate holds.
    pub fn take_while<F>(self, predicate: F) -> Shell<T>
    where
        F: FnMut(&T) -> bool + 'static,
        T: 'static,
    {
        let iter = self.into_boxed();
        Shell::new(iter.take_while(predicate))
    }

    /// Skips the first `n` elements.
    pub fn skip(self, n: usize) -> Shell<T>
    where
        T: 'static,
    {
        let iter = self.into_boxed();
        Shell::new(iter.skip(n))
    }

    /// Skips elements while the predicate holds.
    pub fn skip_while<F>(self, predicate: F) -> Shell<T>
    where
        F: FnMut(&T) -> bool + 'static,
        T: 'static,
    {
        let iter = self.into_boxed();
        Shell::new(iter.skip_while(predicate))
    }

    /// Chains another iterable onto the current stream.
    pub fn chain<I>(self, other: I) -> Shell<T>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: Iterator<Item = T> + 'static,
        T: 'static,
    {
        let iter = self.into_boxed();
        Shell::new(iter.chain(other))
    }

    /// Enumerates elements, pairing them with their index.
    pub fn enumerate(self) -> Shell<(usize, T)>
    where
        T: 'static,
    {
        let iter = self.into_boxed();
        Shell::new(iter.enumerate())
    }

    /// Runs the provided closure for each item while keeping the item in the stream.
    pub fn inspect<F>(self, f: F) -> Shell<T>
    where
        F: FnMut(&T) + 'static,
        T: 'static,
    {
        let iter = self.into_boxed();
        Shell::new(iter.inspect(f))
    }

    /// Collects the stream into a `Vec`.
    pub fn to_vec(self) -> Vec<T> {
        self.into_iter().collect()
    }

    /// Returns the iterator size hint.
    pub fn len_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    /// Collects into any container implementing [`FromIterator`].
    pub fn collect_into<C>(self) -> C
    where
        C: FromIterator<T>,
    {
        self.into_iter().collect()
    }

    /// Groups elements into non-overlapping chunks.
    pub fn chunks(self, size: usize) -> Shell<Vec<T>>
    where
        T: 'static,
    {
        assert!(size > 0, "chunk size must be greater than zero");
        let iter = self.into_boxed();
        Shell::new(ChunkIter::new(iter, size))
    }

    /// Produces sliding windows of size `size`. Requires `T: Clone`.
    pub fn windows(self, size: usize) -> Shell<Vec<T>>
    where
        T: Clone + 'static,
    {
        assert!(size > 0, "window size must be greater than zero");
        let iter = self.into_boxed();
        Shell::new(WindowIter::new(iter, size))
    }

    /// Interleaves this stream with another iterator.
    pub fn interleave<I>(self, other: I) -> Shell<T>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: Iterator<Item = T> + 'static,
        T: 'static,
    {
        let iter_a = self.into_boxed();
        let iter_b: Box<dyn Iterator<Item = T> + 'static> = Box::new(other.into_iter());
        Shell::new(InterleaveIter::new(iter_a, iter_b))
    }

    /// Computes the cartesian product of two streams.
    pub fn product<U, I>(self, other: I) -> Shell<(T, U)>
    where
        T: Clone + 'static,
        U: Clone + 'static,
        I: IntoIterator<Item = U>,
        I::IntoIter: Iterator<Item = U>,
    {
        let iter = self.into_boxed();
        let others = Arc::new(other.into_iter().collect::<Vec<U>>());
        Shell::new(iter.flat_map(move |item| ProductIter::new(item, Arc::clone(&others))))
    }

    /// Zips two streams together.
    pub fn zip<U, I>(self, other: I) -> Shell<(T, U)>
    where
        I: IntoIterator<Item = U>,
        I::IntoIter: Iterator<Item = U> + 'static,
        T: 'static,
        U: 'static,
    {
        let iter = self.into_boxed();
        let other_iter: Box<dyn Iterator<Item = U> + 'static> = Box::new(other.into_iter());
        Shell::new(iter.zip(other_iter))
    }

    /// Joins elements into a string separated by `sep`.
    pub fn join(self, sep: &str) -> String
    where
        T: ToString,
    {
        let mut iter = self.into_iter();
        if let Some(first) = iter.next() {
            let mut acc = first.to_string();
            for elem in iter {
                acc.push_str(sep);
                acc.push_str(&elem.to_string());
            }
            acc
        } else {
            String::new()
        }
    }

    /// Folds the stream left-to-right.
    pub fn fold<U, F>(self, mut acc: U, mut f: F) -> U
    where
        F: FnMut(U, T) -> U,
    {
        for item in self {
            acc = f(acc, item);
        }
        acc
    }

    /// Applies a callback to every value, primarily for side effects.
    pub fn for_each(self, mut f: impl FnMut(T)) {
        for item in self {
            f(item);
        }
    }

    /// Returns only the first occurrence of each item.
    pub fn distinct(self) -> Shell<T>
    where
        T: Eq + std::hash::Hash + Clone + 'static,
    {
        let iter = self.into_boxed();
        Shell::new(DistinctIter::new(iter))
    }

    /// Returns items sorted using their natural order.
    pub fn sorted(self) -> Shell<T>
    where
        T: Ord + 'static,
    {
        let mut vec: Vec<T> = self.into_iter().collect();
        vec.sort();
        Shell::new(vec.into_iter())
    }

    /// Applies a function to chunks of items, yielding results once each chunk is processed.
    ///
    /// This placeholder implementation processes chunks sequentially but exposes
    /// the shape needed to plug in parallelism in the future.
    pub fn chunk_map<F, U>(self, chunk_size: usize, f: F) -> Shell<U>
    where
        F: FnMut(Vec<T>) -> Vec<U> + Send + 'static,
        U: 'static + Send,
        T: Send + 'static,
    {
        assert!(chunk_size > 0, "chunk size must be greater than zero");
        let iter = self.into_boxed();
        Shell::new(ChunkMapIter::new(iter, chunk_size, f))
    }

    /// Applies a function to chunks in parallel when the `parallel` feature is enabled.
    ///
    /// Requires `--features parallel` (brings in the optional `rayon` dependency).
    #[cfg(feature = "parallel")]
    pub fn chunk_map_parallel<F, U>(self, chunk_size: usize, f: F) -> Shell<U>
    where
        F: Fn(Vec<T>) -> Vec<U> + Send + Sync + 'static,
        U: Send + 'static,
        T: Send + 'static,
    {
        use rayon::prelude::*;
        assert!(chunk_size > 0, "chunk size must be greater than zero");
        let data: Vec<T> = self.into_iter().collect();
        let chunks: Vec<Vec<T>> = data
            .chunks(chunk_size)
            .map(|chunk| chunk.to_vec())
            .collect();
        let results: Vec<U> = chunks.into_par_iter().flat_map(|chunk| f(chunk)).collect();
        Shell::new(results.into_iter())
    }

    fn into_boxed(self) -> Box<dyn Iterator<Item = T> + 'static> {
        self.iter
    }
}

impl<T> Iterator for Shell<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl<T> Default for Shell<T>
where
    T: 'static,
{
    fn default() -> Self {
        Shell::empty()
    }
}

impl<T: 'static> std::iter::FromIterator<T> for Shell<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iterable: I) -> Self {
        let data: Vec<T> = iterable.into_iter().collect();
        Shell::new(data.into_iter())
    }
}

struct ChunkIter<T> {
    iter: Box<dyn Iterator<Item = T> + 'static>,
    size: usize,
}

impl<T> ChunkIter<T> {
    fn new(iter: Box<dyn Iterator<Item = T> + 'static>, size: usize) -> Self {
        Self { iter, size }
    }
}

struct WindowIter<T> {
    iter: Box<dyn Iterator<Item = T> + 'static>,
    size: usize,
    buffer: VecDeque<T>,
    initialized: bool,
}

impl<T> WindowIter<T> {
    fn new(iter: Box<dyn Iterator<Item = T> + 'static>, size: usize) -> Self {
        Self {
            iter,
            size,
            buffer: VecDeque::new(),
            initialized: false,
        }
    }
}

impl<T> Iterator for WindowIter<T>
where
    T: Clone,
{
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.initialized {
            while self.buffer.len() < self.size {
                match self.iter.next() {
                    Some(item) => self.buffer.push_back(item),
                    None => break,
                }
            }
            self.initialized = true;
        }
        if self.buffer.len() < self.size {
            return None;
        }
        let window = self.buffer.iter().cloned().collect::<Vec<_>>();
        match self.iter.next() {
            Some(item) => {
                self.buffer.pop_front();
                self.buffer.push_back(item);
            }
            None => {
                self.buffer.pop_front();
            }
        }
        Some(window)
    }
}

struct InterleaveIter<T> {
    a: Box<dyn Iterator<Item = T> + 'static>,
    b: Box<dyn Iterator<Item = T> + 'static>,
    flag: bool,
}

impl<T> InterleaveIter<T> {
    fn new(
        a: Box<dyn Iterator<Item = T> + 'static>,
        b: Box<dyn Iterator<Item = T> + 'static>,
    ) -> Self {
        Self { a, b, flag: false }
    }
}

impl<T> Iterator for InterleaveIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        for _ in 0..2 {
            self.flag = !self.flag;
            if self.flag {
                if let Some(item) = self.a.next() {
                    return Some(item);
                }
            } else if let Some(item) = self.b.next() {
                return Some(item);
            }
        }
        if let Some(item) = self.a.next() {
            return Some(item);
        }
        if let Some(item) = self.b.next() {
            return Some(item);
        }
        None
    }
}

struct ProductIter<T, U> {
    base: T,
    others: Arc<Vec<U>>,
    index: usize,
}

impl<T, U> ProductIter<T, U> {
    fn new(base: T, others: Arc<Vec<U>>) -> Self {
        Self {
            base,
            others,
            index: 0,
        }
    }
}

impl<T, U> Iterator for ProductIter<T, U>
where
    T: Clone,
    U: Clone,
{
    type Item = (T, U);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.others.len() {
            return None;
        }
        let other = self.others[self.index].clone();
        self.index += 1;
        Some((self.base.clone(), other))
    }
}

impl<T> Iterator for ChunkIter<T> {
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut chunk = Vec::with_capacity(self.size);
        for _ in 0..self.size {
            if let Some(item) = self.iter.next() {
                chunk.push(item);
            } else {
                break;
            }
        }
        if chunk.is_empty() { None } else { Some(chunk) }
    }
}

#[cfg(test)]
mod tests {
    use super::Shell;

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
}
struct DistinctIter<T> {
    iter: Box<dyn Iterator<Item = T> + 'static>,
    seen: HashSet<T>,
}

impl<T> DistinctIter<T>
where
    T: Eq + std::hash::Hash,
{
    fn new(iter: Box<dyn Iterator<Item = T> + 'static>) -> Self {
        Self {
            iter,
            seen: HashSet::new(),
        }
    }
}

impl<T> Iterator for DistinctIter<T>
where
    T: Eq + std::hash::Hash + Clone,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .by_ref()
            .find(|item| self.seen.insert(item.clone()))
    }
}
struct ChunkMapIter<T, U, F>
where
    F: FnMut(Vec<T>) -> Vec<U>,
{
    iter: Box<dyn Iterator<Item = T> + 'static>,
    size: usize,
    mapper: F,
    current: Option<IntoIter<U>>,
}

impl<T, U, F> ChunkMapIter<T, U, F>
where
    F: FnMut(Vec<T>) -> Vec<U>,
{
    fn new(iter: Box<dyn Iterator<Item = T> + 'static>, size: usize, mapper: F) -> Self {
        Self {
            iter,
            size,
            mapper,
            current: None,
        }
    }
}

impl<T, U, F> Iterator for ChunkMapIter<T, U, F>
where
    F: FnMut(Vec<T>) -> Vec<U>,
{
    type Item = U;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = &mut self.current {
            if let Some(item) = current.next() {
                return Some(item);
            }
            self.current = None;
        }
        let mut chunk = Vec::with_capacity(self.size);
        for _ in 0..self.size {
            if let Some(item) = self.iter.next() {
                chunk.push(item);
            } else {
                break;
            }
        }
        if chunk.is_empty() {
            return None;
        }
        let mut mapped = (self.mapper)(chunk).into_iter();
        match mapped.next() {
            Some(item) => {
                self.current = Some(mapped);
                Some(item)
            }
            None => self.next(),
        }
    }
}

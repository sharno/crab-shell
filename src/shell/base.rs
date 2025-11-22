use std::iter;
use std::sync::Arc;

use super::iters::{
    ChunkIter, ChunkMapIter, DistinctIter, InterleaveIter, ProductIter, WindowIter,
};

/// A lazy, composable stream of values inspired by Turtle's `Shell`.
///
/// Internally `Shell` is a boxed iterator which keeps the type signature
/// stable when chaining multiple transformations together.
pub struct Shell<T> {
    iter: Box<dyn Iterator<Item = T> + 'static>,
}

/// Iterator wrapper that supports [`DoubleEndedIterator`].
pub struct DoubleEndedShell<T> {
    iter: Box<dyn DoubleEndedIterator<Item = T> + 'static>,
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
        let mut chunks: Vec<Vec<T>> = Vec::new();
        let mut current = Vec::with_capacity(chunk_size);
        for item in self.into_iter() {
            current.push(item);
            if current.len() == chunk_size {
                chunks.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        let results: Vec<U> = chunks.into_par_iter().flat_map(f).collect();
        Shell::new(results.into_iter())
    }

    fn into_boxed(self) -> Box<dyn Iterator<Item = T> + 'static> {
        self.iter
    }
}

#[allow(dead_code)]
impl<T: 'static> DoubleEndedShell<T> {
    /// Wraps any double-ended iterator.
    pub fn new<I>(iter: I) -> Self
    where
        I: DoubleEndedIterator<Item = T> + 'static,
    {
        Self {
            iter: Box::new(iter),
        }
    }

    /// Converts a `Vec<T>` into a double-ended shell.
    pub fn from_vec(vec: Vec<T>) -> Self {
        Self::new(vec.into_iter())
    }

    /// Converts back to a plain [`Shell`], dropping double-ended support.
    pub fn into_shell(self) -> Shell<T> {
        Shell::new(self.iter)
    }
}

impl<T> Iterator for Shell<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl<T> Iterator for DoubleEndedShell<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl<T> DoubleEndedIterator for DoubleEndedShell<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back()
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

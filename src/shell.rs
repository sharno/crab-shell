use std::iter;

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

    /// Converts any iterable type into a `Shell`.
    pub fn from_iter<I>(iterable: I) -> Self
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: Iterator<Item = T> + 'static,
    {
        Self::new(iterable.into_iter())
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

    /// Collects into any container implementing [`FromIterator`].
    pub fn collect_into<C>(self) -> C
    where
        C: FromIterator<T>,
    {
        self.into_iter().collect()
    }

    /// Groups elements into chunks of the provided size.
    pub fn chunks(self, size: usize) -> Shell<Vec<T>>
    where
        T: 'static,
    {
        assert!(size > 0, "chunk size must be greater than zero");
        let iter = self.into_boxed();
        Shell::new(ChunkIter::new(iter, size))
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
        let other_iter: Box<dyn Iterator<Item = U> + 'static> =
            Box::new(other.into_iter());
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
    pub fn fold<U, F>(mut self, mut acc: U, mut f: F) -> U
    where
        F: FnMut(U, T) -> U,
    {
        while let Some(item) = self.next() {
            acc = f(acc, item);
        }
        acc
    }

    /// Applies a callback to every value, primarily for side effects.
    pub fn for_each(mut self, mut f: impl FnMut(T)) {
        while let Some(item) = self.next() {
            f(item);
        }
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

struct ChunkIter<T> {
    iter: Box<dyn Iterator<Item = T> + 'static>,
    size: usize,
}

impl<T> ChunkIter<T> {
    fn new(iter: Box<dyn Iterator<Item = T> + 'static>, size: usize) -> Self {
        Self { iter, size }
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
        if chunk.is_empty() {
            None
        } else {
            Some(chunk)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Shell;

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

        let zipped: Vec<_> =
            Shell::from_iter(["a".to_string(), "b".to_string()])
                .zip(["x", "y"])
                .collect();
        assert_eq!(
            zipped,
            vec![
                ("a".to_string(), "x"),
                ("b".to_string(), "y"),
            ]
        );
    }
}

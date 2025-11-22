use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    vec::IntoIter,
};

pub struct ChunkIter<T> {
    iter: Box<dyn Iterator<Item = T> + 'static>,
    size: usize,
}

impl<T> ChunkIter<T> {
    pub fn new(iter: Box<dyn Iterator<Item = T> + 'static>, size: usize) -> Self {
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
        if chunk.is_empty() { None } else { Some(chunk) }
    }
}

impl<T> std::iter::FusedIterator for ChunkIter<T> {}

pub struct WindowIter<T> {
    iter: Box<dyn Iterator<Item = T> + 'static>,
    size: usize,
    buffer: VecDeque<T>,
    initialized: bool,
}

impl<T> WindowIter<T> {
    pub fn new(iter: Box<dyn Iterator<Item = T> + 'static>, size: usize) -> Self {
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

impl<T> std::iter::FusedIterator for WindowIter<T> where T: Clone {}

pub struct InterleaveIter<T> {
    a: Box<dyn Iterator<Item = T> + 'static>,
    b: Box<dyn Iterator<Item = T> + 'static>,
    flag: bool,
}

impl<T> InterleaveIter<T> {
    pub fn new(
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

pub struct ProductIter<T, U> {
    base: T,
    others: Arc<Vec<U>>,
    index: usize,
}

impl<T, U> ProductIter<T, U> {
    pub fn new(base: T, others: Arc<Vec<U>>) -> Self {
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

pub struct DistinctIter<T> {
    iter: Box<dyn Iterator<Item = T> + 'static>,
    seen: HashSet<T>,
}

impl<T> DistinctIter<T>
where
    T: Eq + std::hash::Hash,
{
    pub fn new(iter: Box<dyn Iterator<Item = T> + 'static>) -> Self {
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

pub struct ChunkMapIter<T, U, F>
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
    pub fn new(iter: Box<dyn Iterator<Item = T> + 'static>, size: usize, mapper: F) -> Self {
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
        loop {
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
            if let Some(item) = mapped.next() {
                self.current = Some(mapped);
                return Some(item);
            }
        }
    }
}

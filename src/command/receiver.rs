use std::sync::mpsc::Receiver;

pub struct ReceiverIter<T> {
    rx: Receiver<T>,
}

impl<T> ReceiverIter<T> {
    pub fn new(rx: Receiver<T>) -> Self {
        Self { rx }
    }
}

impl<T> Iterator for ReceiverIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.rx.recv().ok()
    }
}

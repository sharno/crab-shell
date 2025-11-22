use crate::{Error, Result};

use std::{
    fmt,
    io::{Read, Write},
    process::Child,
    sync::{Arc, Mutex},
    thread,
};

pub type StdinJoinHandle = thread::JoinHandle<std::io::Result<()>>;

pub enum StdinSource {
    Bytes(Vec<u8>),
    Reader(Arc<Mutex<Option<Box<dyn Read + Send>>>>),
}

impl StdinSource {
    pub fn reader<R>(reader: R) -> Self
    where
        R: Read + Send + 'static,
    {
        StdinSource::Reader(Arc::new(Mutex::new(Some(Box::new(reader)))))
    }

    pub fn try_clone(&self) -> Option<Self> {
        match self {
            StdinSource::Bytes(data) => Some(StdinSource::Bytes(data.clone())),
            StdinSource::Reader(_) => None,
        }
    }
}

impl fmt::Debug for StdinSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StdinSource::Bytes(data) => f.debug_tuple("Bytes").field(&data.len()).finish(),
            StdinSource::Reader(_) => f.write_str("Reader(..)"),
        }
    }
}

pub fn feed_child_stdin(
    child: &mut Child,
    source: &Option<StdinSource>,
) -> Result<Option<StdinJoinHandle>> {
    match source {
        Some(StdinSource::Bytes(data)) => {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| Error::Io(std::io::Error::other("missing stdin pipe")))?;
            stdin.write_all(data)?;
            Ok(None)
        }
        Some(StdinSource::Reader(shared)) => {
            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| Error::Io(std::io::Error::other("missing stdin pipe")))?;
            let reader = {
                let mut guard = shared.lock().unwrap();
                guard.take().ok_or_else(|| {
                    Error::Io(std::io::Error::other("stdin reader already consumed"))
                })?
            };
            let handle = thread::spawn(move || {
                let mut reader = reader;
                let mut stdin = stdin;
                std::io::copy(&mut reader, &mut stdin)?;
                stdin.flush()?;
                Ok(())
            });
            Ok(Some(handle))
        }
        None => Ok(None),
    }
}

pub fn wait_stdin_writer(handle: Option<StdinJoinHandle>) -> Result<()> {
    if let Some(handle) = handle {
        let result = handle.join().map_err(|err| {
            Error::Io(std::io::Error::other(format!(
                "stdin writer task panicked: {err:?}"
            )))
        })?;
        result.map_err(Error::Io)?;
    }
    Ok(())
}

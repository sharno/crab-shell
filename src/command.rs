pub mod builder;
pub mod pipeline;
mod receiver;
mod stdin;

pub use builder::{Command, CommandOutput, cmd, sh};
pub use pipeline::Pipeline;

pub(crate) use receiver::ReceiverIter;
pub(crate) use stdin::{StdinJoinHandle, StdinSource, feed_child_stdin, wait_stdin_writer};

#[cfg(test)]
mod tests;

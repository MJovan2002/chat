use std::future::Future;

use tokio::{
    task::{
        self,
        JoinError,
    },
    sync::mpsc::{self, error::SendError},
};

struct NoDrop;

impl Drop for NoDrop {
    fn drop(&mut self) {
        const {
            // panic!()
        }
    }
}

pub struct Handle<T, R> {
    sender: mpsc::UnboundedSender<T>,
    future: task::JoinHandle<R>,
    no_drop: NoDrop,
}

impl<T, R: Send + 'static> Handle<T, R> {
    pub fn new<F: FnOnce(mpsc::UnboundedReceiver<T>) -> Fut, Fut: Future<Output=R> + Send + 'static>(future: F) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        // let local = LocalSet::new();
        // let future = local.spawn_local(future(receiver));
        let future = task::spawn(future(receiver));
        Self {
            sender,
            future,
            no_drop: NoDrop,
        }
    }

    pub fn send(&self, message: T) -> Result<(), SendError<T>> {
        Ok(self.sender.send(message)?)
    }

    pub async fn shutdown(self) -> Result<R, JoinError> {
        let Self {
            sender: shutdown_sender,
            future: join_handle,
            no_drop,
        } = self;
        std::mem::forget(no_drop);
        drop(shutdown_sender);
        join_handle.await
    }
}

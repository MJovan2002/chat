#![feature(generic_const_exprs)]
#![feature(iter_array_chunks)]
#![feature(int_roundings)]
#![deny(unused_import_braces)]

extern crate core;

use std::future::Future;
use tokio::task;
use tokio::task::JoinHandle;
use crate::logger::Logger;

pub mod client;
pub mod message;
pub mod stream;
pub mod db;
pub mod server;
pub mod logger;

fn spawn_and_handle<F: Future<Output=anyhow::Result<()>> + Send + 'static, L: Logger + Send + 'static>(fut: F, logger: L) -> JoinHandle<()> {
    task::spawn(async move {
        if let Err(e) = fut.await {
            logger.error(&format!("{e}"))
        }
    })
}

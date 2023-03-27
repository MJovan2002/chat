use std::fmt::Debug;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;

pub enum LogLevel {
    Fatal,
    Error,
    Warn,
    Info,
}

pub trait Logger: Clone {
    fn fatal<S: Debug>(&self, message: S);

    fn error<S: Debug>(&self, message: S);

    fn warn<S: Debug>(&self, message: S);

    fn info<S: Debug>(&self, message: S);

    fn log<S: Debug>(&self, level: LogLevel, message: S) {
        (match level {
            LogLevel::Fatal => Self::fatal,
            LogLevel::Error => Self::error,
            LogLevel::Warn => Self::warn,
            LogLevel::Info => Self::info,
        })(self, message);
    }
}

#[derive(Default, Clone)]
pub struct StdioLogger;

impl StdioLogger {
    fn log<S: Debug>(&self, prefix: &str, message: S) {
        println!("[{prefix}] {message:?}");
    }
}

impl Logger for StdioLogger {
    fn fatal<S: Debug>(&self, message: S) {
        self.log("FATAL", message)
    }

    fn error<S: Debug>(&self, message: S) {
        self.log("ERROR", message)
    }

    fn warn<S: Debug>(&self, message: S) {
        self.log("WARN", message)
    }

    fn info<S: Debug>(&self, message: S) {
        self.log("INFO", message)
    }
}

pub struct FileLogger {
    inner: Arc<Mutex<File>>,
}

impl FileLogger {
    pub fn new(file: File) -> Self {
        Self { inner: Arc::new(Mutex::new(file)) }
    }
}

impl Clone for FileLogger {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl FileLogger {
    fn log<S: Debug>(&self, prefix: &str, message: S) {
        writeln!(self.inner.lock().unwrap(), "[{prefix}] {message:?}").unwrap();
    }
}

impl Logger for FileLogger {
    fn fatal<S: Debug>(&self, message: S) {
        self.log("FATAL", message)
    }

    fn error<S: Debug>(&self, message: S) {
        self.log("ERROR", message)
    }

    fn warn<S: Debug>(&self, message: S) {
        self.log("WARN", message)
    }

    fn info<S: Debug>(&self, message: S) {
        self.log("INFO", message)
    }
}
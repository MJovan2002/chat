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

pub trait Logger {
    fn fatal(&self, message: &str);

    fn error(&self, message: &str);

    fn warn(&self, message: &str);

    fn info(&self, message: &str);

    fn log(&self, level: LogLevel, message: &str) {
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
    fn log(&self, prefix: &str, message: &str) {
        println!("[{prefix}] {message}");
    }
}

impl Logger for StdioLogger {
    fn fatal(&self, message: &str) {
        self.log("FATAL", message)
    }

    fn error(&self, message: &str) {
        self.log("ERROR", message)
    }

    fn warn(&self, message: &str) {
        self.log("WARN", message)
    }

    fn info(&self, message: &str) {
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
    fn log(&self, prefix: &str, message: &str) {
        writeln!(self.inner.lock().unwrap(), "[{prefix}] {message}").unwrap();
    }
}

impl Logger for FileLogger {
    fn fatal(&self, message: &str) {
        self.log("FATAL", message)
    }

    fn error(&self, message: &str) {
        self.log("ERROR", message)
    }

    fn warn(&self, message: &str) {
        self.log("WARN", message)
    }

    fn info(&self, message: &str) {
        self.log("INFO", message)
    }
}
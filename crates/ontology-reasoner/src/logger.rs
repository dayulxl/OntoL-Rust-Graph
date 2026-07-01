//! 零依赖迷你日志器。
//!
//! 实现 `log::Log` trait，输出到 stderr，支持 `RUST_LOG` 环境变量控制级别。

use log::{Level, LevelFilter, Log, Metadata, Record};

struct MiniLogger;

impl Log for MiniLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let level = record.level();
        let prefix = match level {
            Level::Error => "[ERROR]",
            Level::Warn => "[WARN] ",
            Level::Info => "[INFO] ",
            Level::Debug => "[DEBUG]",
            Level::Trace => "[TRACE]",
        };
        // 写 stderr + flush，确保终端立即可见
        let msg = format!("{} {}\n", prefix, record.args());
        use std::io::Write;
        let _ = std::io::stderr().write_all(msg.as_bytes());
        let _ = std::io::stderr().flush();
    }

    fn flush(&self) {
        use std::io::Write;
        let _ = std::io::stderr().flush();
    }
}

fn max_level() -> LevelFilter {
    if let Ok(val) = std::env::var("RUST_LOG") {
        match val.to_lowercase().as_str() {
            "error" => LevelFilter::Error,
            "warn" => LevelFilter::Warn,
            "info" => LevelFilter::Info,
            "debug" => LevelFilter::Debug,
            "trace" => LevelFilter::Trace,
            "off" => LevelFilter::Off,
            _ => LevelFilter::Info,
        }
    } else {
        LevelFilter::Info
    }
}

/// 初始化日志器。在 main 函数开头调用一次即可。
pub fn init() {
    log::set_max_level(max_level());
    let _ = log::set_logger(&MiniLogger);
}

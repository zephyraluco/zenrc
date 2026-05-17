//! # zenrc-log
//!
//! 基于 [`tracing`] 的结构化日志记录，支持控制台输出和滚动文件输出。
//!
//! ## 快速上手
//!
//! ```no_run
//! use zenrc_log::{SubscriberBuilder, Level};
//!
//! // 只输出到控制台
//! SubscriberBuilder::new().with_level(Level::DEBUG).init();
//!
//! // 同时输出到控制台和滚动日志文件
//! SubscriberBuilder::new()
//!     .with_path("/var/log/app/app.log")
//!     .with_level(Level::INFO)
//!     .init();
//!
//! zenrc_log::info!("hello from zenrc-log");
//! ```

pub mod appender;
pub mod formatter;
use std::path::Path;

use appender::builder::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::FormatEvent;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::Registry;
use tracing_subscriber::util::SubscriberInitExt;

use crate::formatter::LogFormatter;

pub use tracing::Level;
pub use tracing::{debug, error, info, trace, warn};

/// 日志滚动周期。
///
/// 传入 [`SubscriberBuilder::with_rotation`] 控制滚动时机。
pub enum Period {
    Minute,
    Hour,
    Day,
    Month,
    NEVER,
}
impl Into<Rotation> for Period {
    fn into(self) -> Rotation {
        match self {
            Period::Minute => Rotation::MINUTELY,
            Period::Hour => Rotation::HOURLY,
            Period::Day => Rotation::DAILY,
            Period::Month => Rotation::MONTHLY,
            Period::NEVER => Rotation::NEVER,
        }
    }
}
/// 日志订阅器构建器。
///
/// 采用链式 Builder 模式配置日志选项，最后调用 [`init`](Self::init) 初始化全局订阅者。
///
/// - 未设置路径时：只输出到标准展示。
/// - 设置路径时：同时输出到滚动文件。
pub struct SubscriberBuilder<E = LogFormatter> {
    event_formatter: E,
    level: Level,
    directory: String,
    appender_builder: appender::builder::Builder,
}

impl SubscriberBuilder {
    /// 使用默认日志格式器创建构建器，日志级别默认为 `INFO`。
    pub fn new() -> Self {
        SubscriberBuilder {
            event_formatter: LogFormatter,
            level: Level::INFO,
            directory: String::new(),
            appender_builder: RollingFileAppender::builder(),
        }
    }
}

impl<E> SubscriberBuilder<E>
where
    E: FormatEvent<Registry, fmt::format::DefaultFields> + Send + Sync + 'static,
{
    /// 替换默认事件格式化器。
    pub fn with_event_format(self, formatter: E) -> Self {
        SubscriberBuilder {
            event_formatter: formatter,
            ..self
        }
    }
    /// 设置日志级别过滤器。
    pub fn with_level(self, level: Level) -> Self {
        SubscriberBuilder { level, ..self }
    }
    /// 设置滚动日志文件路径（如 `/var/log/app/app.log`）。
    ///
    /// 自动拆分目录和文件名。
    pub fn with_path(self, path: impl Into<String>) -> Self {
        let path = path.into();
        let file_name = Path::new(&path).file_name().unwrap().to_str().unwrap();
        let directory = Path::new(&path).parent().unwrap().to_str().unwrap();
        SubscriberBuilder {
            directory: directory.into(),
            appender_builder: self.appender_builder.filename(file_name),
            ..self
        }
    }
    /// 设置滚动周期，默认永不滚动。
    pub fn with_rotation(self, period: Period) -> Self {
        SubscriberBuilder {
            appender_builder: self.appender_builder.rotation(period.into()),
            ..self
        }
    }
    /// 设置单个指标的文件滚动最大数，超出后删除最旧日志。
    pub fn with_max_log_files(self, max: usize) -> Self {
        SubscriberBuilder {
            appender_builder: self.appender_builder.max_log_files(max),
            ..self
        }
    }
    /// 为指定模块名（`target`）设置独立的滚动文件路径（`filename`）。
    pub fn with_filter(
        self,
        target: impl Into<String>,
        filename: impl Into<String>,
    ) -> SubscriberBuilder<E> {
        let target = target.into();
        let filename = filename.into();
        Self {
            appender_builder: self.appender_builder.filter(target, filename),
            ..self
        }
    }

    /// 初始化全局 [`tracing`] 订阅者。
    ///
    /// 应在程序入口调用一次，重复调用会 panic。
    pub fn init(self) {
        if self.directory.is_empty() {
            let filter = tracing_subscriber::filter::LevelFilter::from_level(self.level);
            let layer = fmt::layer()
                .event_format(self.event_formatter)
                .with_ansi(false);
            tracing_subscriber::registry()
                .with(layer)
                .with(filter)
                .init();
        } else {
            // let file_name = Path::new(&self.path).file_name().unwrap().to_str().unwrap();
            // let dir = Path::new(&self.path).parent().unwrap().to_str().unwrap();
            let file_appender = self
                .appender_builder
                .build(self.directory)
                .expect("failed to initialize rolling file appender");

            let layer = fmt::layer()
                .event_format(self.event_formatter)
                .with_writer(file_appender)
                .with_ansi(false);
            let filter = tracing_subscriber::filter::LevelFilter::from_level(self.level);
            tracing_subscriber::registry()
                .with(layer)
                .with(filter)
                .init();
        }
    }
}

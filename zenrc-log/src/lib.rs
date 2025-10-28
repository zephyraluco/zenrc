pub mod appender;
pub mod filter;
pub mod formatter;
use std::path::Path;

use appender::builder::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::FormatEvent;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::Registry;
use tracing_subscriber::util::SubscriberInitExt;

use crate::formatter::LogFormatter;

pub use tracing::{info, warn, error, debug, trace};
pub use tracing::Level;


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
pub struct SubscriberBuilder<E = LogFormatter> {
    event_formatter: E,
    level: Level,
    directory: String,
	appender_builder: appender::builder::Builder,
}

impl SubscriberBuilder {
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
    pub fn with_event_format(self, formatter: E) -> Self {
        SubscriberBuilder {
            event_formatter: formatter,
            ..self
        }
    }
    pub fn with_level(self, level: Level) -> Self {
        SubscriberBuilder {
            level,
            ..self
        }
    }
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
    pub fn with_rotation(self, period: Period) -> Self {
        SubscriberBuilder {
            appender_builder: self.appender_builder.rotation(period.into()),
            ..self
        }
    }
    pub fn with_max_log_files(self, max: usize) -> Self {
        SubscriberBuilder {
            appender_builder: self.appender_builder.max_log_files(max),
            ..self
        }
    }
    pub fn with_filter(self, target: impl Into<String>, filename: impl Into<String>) -> SubscriberBuilder<E> {
        let target = target.into();
        let filename = filename.into();
        Self {
            appender_builder: self.appender_builder.filter(target, filename),
            ..self
        }
    }

    pub fn init(self) {
        if self.directory.is_empty() {
            let filter = tracing_subscriber::filter::LevelFilter::from_level(self.level);
            let layer = fmt::layer().event_format(self.event_formatter).with_ansi(false);
            tracing_subscriber::registry().with(layer).with(filter).init();
        } else {
            // let file_name = Path::new(&self.path).file_name().unwrap().to_str().unwrap();
            // let dir = Path::new(&self.path).parent().unwrap().to_str().unwrap();
            let file_appender = 
			// RollingFileAppender::builder()
            //     .rotation(self.period.into())
            //     .max_log_files(self.max_log_files)
            //     .filename(file_name)
			// 	.filter(self.filters)
			self.appender_builder
                .build(self.directory)
                .expect("failed to initialize rolling file appender");


            let layer = fmt::layer()
                .event_format(self.event_formatter)
                .with_writer(file_appender)
                .with_ansi(false);
            let filter = tracing_subscriber::filter::LevelFilter::from_level(self.level);
            tracing_subscriber::registry().with(layer).with(filter).init();
        }
    }
}

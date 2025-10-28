use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use thiserror::Error;
use time::{Date, Duration, OffsetDateTime, Time, UtcOffset, format_description};
use tracing::Metadata;

use super::sync::{RwLock, RwLockReadGuard};

#[derive(Debug)]
pub struct Builder {
    pub(super) rotation: Rotation,
    pub(super) prefix: String,
    pub(super) max_files: Option<usize>,
    pub(super) filters: Option<HashMap<String, String>>,
}

/// Errors returned by [`Builder::build`].
#[derive(Error, Debug)]
#[error("{context}: {source}")]
pub struct InitError {
    context: &'static str,
    #[source]
    source: io::Error,
}

impl InitError {
    pub(crate) fn ctx(context: &'static str) -> impl FnOnce(io::Error) -> Self {
        move |source| Self {
            context,
            source,
        }
    }
}

impl From<time::error::IndeterminateOffset> for InitError {
    fn from(error: time::error::IndeterminateOffset) -> Self {
        Self {
            context: "failed to determine local timezone",
            source: io::Error::new(io::ErrorKind::Other, error),
        }
    }
}

impl Builder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rotation: Rotation::NEVER,
            prefix: String::new(),
            // suffix: None,
            max_files: None,
            filters: None,
        }
    }

    #[must_use]
    pub fn rotation(self, rotation: Rotation) -> Self {
        Self {
            rotation,
            ..self
        }
    }

    #[must_use]
    pub fn filename(self, prefix: impl Into<String>) -> Self {
        let prefix = prefix.into();
        Self {
            prefix,
            ..self
        }
    }

    #[must_use]
    pub fn filter(self, target: impl Into<String>, filename: impl Into<String>) -> Self {
        let target = target.into();
        let filename = filename.into();
        let mut filters = self.filters.unwrap_or_else(HashMap::new);
        filters.insert(target, filename);
        Self {
            filters: Some(filters),
            ..self
        }
    }

    #[must_use]
    pub fn max_log_files(self, n: usize) -> Self {
        Self {
            max_files: Some(n),
            ..self
        }
    }

    pub fn build(&self, directory: impl AsRef<Path>) -> Result<RollingFileAppender, InitError> {
        RollingFileAppender::from_builder(self, directory)
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

/// 以下为rolling.rs的内容
///
#[derive(Debug)]
pub struct WriterMeta {
    log_directory: PathBuf,
    log_filename: String,
    // date_format: Vec<format_description::FormatItem<'static>>,
    crate_time: RwLock<OffsetDateTime>,
    max_files: Option<usize>,
    writer: RwLock<File>,
}

impl WriterMeta {
    fn new(
        directory: impl AsRef<Path>,
        log_filename: String,
        // rotation: Rotation,
        max_files: Option<usize>,
    ) -> Result<Self, InitError> {
        let log_directory = directory.as_ref().to_path_buf();
        // let date_format = rotation.date_format();

        let writer: RwLock<File> =
            RwLock::new(create_writer(log_directory.as_ref(), &log_filename)?);
        let crate_time = OffsetDateTime::from(writer.read().metadata().unwrap().created().unwrap())
            .to_offset(UtcOffset::local_offset_at(OffsetDateTime::now_utc()).unwrap());
        Ok(Self {
            log_directory,
            log_filename,
            // date_format,
            crate_time: RwLock::new(crate_time),
            max_files,
            writer,
        })
    }

    pub(crate) fn join_date(
        &self,
        date: &OffsetDateTime,
        date_format: &Vec<format_description::FormatItem<'static>>,
    ) -> String {
        let date = date
            .format(date_format)
            .expect("Unable to format OffsetDateTime; this is a bug in tracing-appender");

        format!("{}.{}", self.log_filename, date)
    }

    //清理旧日志文件
    fn prune_old_logs(&self, max_files: usize) {
        let files = fs::read_dir(&self.log_directory).map(|dir| {
            dir.filter_map(|entry| {
                let entry = entry.ok()?;
                let metadata = entry.metadata().ok()?;

                // the appender only creates files, not directories or symlinks,
                // so we should never delete a dir or symlink.
                if !metadata.is_file() {
                    return None;
                }

                let filename = entry.file_name();
                // if the filename is not a UTF-8 string, skip it.
                let filename = filename.to_str()?;
                if !filename.starts_with(&self.log_filename) {
                    return None;
                }

                let created = metadata.created().ok()?;
                Some((entry, created))
            })
            .collect::<Vec<_>>()
        });

        let mut files = match files {
            Ok(files) => files,
            Err(error) => {
                eprintln!("Error reading the log directory/files: {}", error);
                return;
            }
        };
        if files.len() < max_files {
            return;
        }

        // sort the files by their creation timestamps.
        files.sort_by_key(|(_, created_at)| *created_at);

        // delete files, so that (n-1) files remain, because we will create another log file
        for (file, _) in files.iter().take(files.len() - (max_files - 1)) {
            if let Err(error) = fs::remove_file(file.path()) {
                eprintln!(
                    "Failed to remove old log file {}: {}",
                    file.path().display(),
                    error
                );
            }
        }
    }

    fn refresh_writer(
        &self,
        file: &mut File,
        date_format: &Vec<format_description::FormatItem<'static>>,
    ) {
        let filename = self.join_date(&self.crate_time.read(), date_format);

        if let Some(max_files) = self.max_files {
            self.prune_old_logs(max_files);
        }
        fs::rename(
            self.log_directory.join(&self.log_filename),
            self.log_directory.join(filename),
        )
        .unwrap();
        match create_writer(&self.log_directory, &self.log_filename) {
            Ok(new_file) => {
                if let Err(err) = file.flush() {
                    eprintln!("Couldn't flush previous writer: {}", err);
                }
                *self.crate_time.write() =
                    get_current_time(new_file.metadata().unwrap().created().unwrap());
                *file = new_file;
            }
            Err(err) => eprintln!("Couldn't create writer for logs: {}", err),
        }
    }

    // 检查是否需要滚动日志文件
    fn should_rollover(&self, rotation: &Rotation) -> bool {
        let now = OffsetDateTime::now_local().expect("Failed to get local time");
        // Should we try to roll over the log file?
        if let Some(time) = rotation.next_date(&self.crate_time.read()) {
            if now >= time {
                return true;
            }
        }
        false
    }
}
pub struct RollingFileAppender {
    rotation: Rotation,
    date_format: Vec<format_description::FormatItem<'static>>,
    writers: HashMap<String, WriterMeta>,
}

#[derive(Debug)]
pub struct RollingWriter<'a>(RwLockReadGuard<'a, File>);

// === impl RollingFileAppender ===

impl RollingFileAppender {
    pub fn new(
        rotation: Rotation,
        directory: impl AsRef<Path>,
        filename_prefix: impl AsRef<Path>,
    ) -> RollingFileAppender {
        let filename = filename_prefix
            .as_ref()
            .to_str()
            .expect("filename prefix must be a valid UTF-8 string");
        Self::builder()
            .rotation(rotation)
            .filename(filename)
            .build(directory)
            .expect("initializing rolling file appender failed")
    }

    #[must_use]
    pub fn builder() -> Builder {
        Builder::new()
    }

    fn from_builder(builder: &Builder, directory: impl AsRef<Path>) -> Result<Self, InitError> {
        let Builder {
            rotation,
            prefix,
            // suffix,
            max_files,
            filters,
        } = builder;

        let directory = directory.as_ref().to_path_buf();

        // 创建默认的writer
        let mut writers = HashMap::new();
        let writer_meta = WriterMeta::new(
            directory.clone(),
            prefix.clone(),
            // rotation.clone(),
            *max_files,
        )?;
        writers.insert("default".to_string(), writer_meta);

        // 创建过滤的writer
        if let Some(filters) = filters {
            for (target, filename) in filters {
                let writer = WriterMeta::new(
                    directory.clone(),
                    filename.clone(),
                    // rotation.clone(),
                    *max_files,
                )?;
                writers.insert(target.clone(), writer);
            }
        }

        //删除旧日志
        if max_files.is_some() {
            for writer in writers.values() {
                if *writer.crate_time.read()
                    > rotation
                        .next_date(&get_current_time(
                            writer.writer.read().metadata().unwrap().created().unwrap(),
                        ))
                        .unwrap()
                {
                    writer.refresh_writer(&mut writer.writer.write(), &rotation.date_format());
                }
            }
        }

        Ok(Self {
            rotation: rotation.clone(),
            date_format: rotation.date_format(),
            writers,
        })
    }
}

// 手动写入
// impl io::Write for RollingFileAppender {
//     fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
//         let now = OffsetDateTime::now_local().expect("Failed to get local time");
//         let writer = self.writer.get_mut();
//         if let Some(current_time) = self.state.should_rollover(now) {
//             let _did_cas = self.state.advance_date(now, current_time);
//             debug_assert!(
//                 _did_cas,
//                 "if we have &mut access to the appender, no other thread can have advanced the timestamp..."
//             );
//             self.state.refresh_writer(now, writer);
//         }
//         writer.write(buf)
//     }

//     fn flush(&mut self) -> io::Result<()> {
//         self.writer.get_mut().flush()
//     }
// }

/// tracing_subscriber日志事件触发时调用
impl<'a> tracing_subscriber::fmt::writer::MakeWriter<'a> for RollingFileAppender {
    type Writer = RollingWriter<'a>;

    //? 未调用的函数
    fn make_writer(&'a self) -> Self::Writer {
        RollingWriter(self.writers.get("default").unwrap().writer.read())
    }

    fn make_writer_for(&'a self, meta: &Metadata<'_>) -> Self::Writer {
        if let Some(target) = self.writers.get(meta.target()) {
            let writer = &target.writer;
            if target.should_rollover(&self.rotation) {
                target.refresh_writer(&mut writer.write(), &self.date_format);
            }
            return RollingWriter(writer.read());
        }
        let writer = &self.writers.get("default").unwrap().writer;
        if self.writers.get("default").unwrap().should_rollover(&self.rotation)
        {
            self.writers
                .get("default")
                .unwrap()
                .refresh_writer(&mut writer.write(), &self.date_format);
        }
        RollingWriter(self.writers.get("default").unwrap().writer.read())
    }
}

impl fmt::Debug for RollingFileAppender {
    // This manual impl is required because of the `now` field (only present
    // with `cfg(test)`), which is not `Debug`...
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RollingFileAppender")
            .field("rotation", &self.rotation)
            .field("writers", &self.writers)
            .finish()
    }
}

pub fn minutely(directory: impl AsRef<Path>, file_name: impl AsRef<Path>) -> RollingFileAppender {
    RollingFileAppender::new(Rotation::MINUTELY, directory, file_name)
}

pub fn hourly(directory: impl AsRef<Path>, file_name: impl AsRef<Path>) -> RollingFileAppender {
    RollingFileAppender::new(Rotation::HOURLY, directory, file_name)
}

pub fn daily(directory: impl AsRef<Path>, file_name: impl AsRef<Path>) -> RollingFileAppender {
    RollingFileAppender::new(Rotation::DAILY, directory, file_name)
}

pub fn monthly(directory: impl AsRef<Path>, file_name: impl AsRef<Path>) -> RollingFileAppender {
    RollingFileAppender::new(Rotation::MONTHLY, directory, file_name)
}

pub fn never(directory: impl AsRef<Path>, file_name: impl AsRef<Path>) -> RollingFileAppender {
    RollingFileAppender::new(Rotation::NEVER, directory, file_name)
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Rotation(RotationKind);

#[derive(Clone, Eq, PartialEq, Debug)]
enum RotationKind {
    Minutely,
    Hourly,
    Daily,
    Monthly,
    Never,
}

impl Rotation {
    /// Provides an minutely rotation
    pub const MINUTELY: Self = Self(RotationKind::Minutely);
    /// Provides an hourly rotation
    pub const HOURLY: Self = Self(RotationKind::Hourly);
    /// Provides a daily rotation
    pub const DAILY: Self = Self(RotationKind::Daily);
    /// Provides a monthly rotation
    pub const MONTHLY: Self = Self(RotationKind::Monthly);
    /// Provides a rotation that never rotates.
    pub const NEVER: Self = Self(RotationKind::Never);

    pub(crate) fn next_date(&self, current_date: &OffsetDateTime) -> Option<OffsetDateTime> {
        let unrounded_next_date = match *self {
            Rotation::MINUTELY => {
                let time = Time::from_hms(current_date.hour(), current_date.minute(), 0)
                    .expect("Invalid time; this is a bug in tracing-appender");
                current_date.replace_time(time) + Duration::minutes(1)
            }
            Rotation::HOURLY => {
                let time = Time::from_hms(current_date.hour(), 0, 0)
                    .expect("Invalid time; this is a bug in tracing-appender");
                current_date.replace_time(time) + Duration::hours(1)
            }
            Rotation::DAILY => {
                let time = Time::from_hms(0, 0, 0)
                    .expect("Invalid time; this is a bug in tracing-appender");
                current_date.replace_time(time) + Duration::days(1)
            }
            Rotation::MONTHLY => {
                // 当前年月
                let year = current_date.year();
                let month = current_date.month();

                // 计算下个月和对应年份
                let (next_year, next_month) = if month == time::Month::December {
                    (year + 1, time::Month::January)
                } else {
                    (year, month.next())
                };
                Date::from_calendar_date(next_year, next_month, 1)
                    .expect("Invalid date; this is a bug in tracing-appender")
                    .with_time(Time::MIDNIGHT)
                    .assume_offset(current_date.offset()) // 保持当前时区偏移
            }
            Rotation::NEVER => return None,
        };
        Some(unrounded_next_date)
        // Some(self.round_date(&unrounded_next_date))
    }

    // // note that this method will panic if passed a `Rotation::NEVER`.
    // pub(crate) fn round_date(&self, date: &OffsetDateTime) -> OffsetDateTime {
    //     match *self {
    //         Rotation::MINUTELY => {
    //             let time = Time::from_hms(date.hour(), date.minute(), 0)
    //                 .expect("Invalid time; this is a bug in tracing-appender");
    //             date.replace_time(time)
    //         }
    //         Rotation::HOURLY => {
    //             let time = Time::from_hms(date.hour(), 0, 0)
    //                 .expect("Invalid time; this is a bug in tracing-appender");
    //             date.replace_time(time)
    //         }
    //         Rotation::DAILY => {
    //             let time = Time::from_hms(0, 0, 0)
    //                 .expect("Invalid time; this is a bug in tracing-appender");
    //             date.replace_time(time)
    //         }
    //         // Rotation::NEVER is impossible to round.
    //         Rotation::NEVER => {
    //             unreachable!("Rotation::NEVER is impossible to round.")
    //         }
    //     }
    // }

    fn date_format(&self) -> Vec<format_description::FormatItem<'static>> {
        match *self {
            Rotation::MINUTELY => format_description::parse("[year]-[month]-[day]-[hour]-[minute]"),
            Rotation::HOURLY => format_description::parse("[year]-[month]-[day]-[hour]"),
            Rotation::DAILY => format_description::parse("[year]-[month]-[day]"),
            Rotation::MONTHLY => format_description::parse("[year]-[month]"),
            Rotation::NEVER => format_description::parse("[year]-[month]-[day]"),
        }
        .expect("Unable to create a formatter; this is a bug in tracing-appender")
    }
}

// === impl RollingWriter ===

impl io::Write for RollingWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self.0).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self.0).flush()
    }
}

fn create_writer(directory: &Path, filename: &str) -> Result<File, InitError> {
    let path = directory.join(filename);
    let mut open_options = OpenOptions::new();
    open_options.append(true).create(true);

    let new_file = open_options.open(path.as_path());
    if new_file.is_err() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(InitError::ctx("failed to create log directory"))?;
            return open_options
                .open(path)
                .map_err(InitError::ctx("failed to create initial log file"));
        }
    }

    new_file.map_err(InitError::ctx("failed to create initial log file"))
}

fn get_current_time(time: SystemTime) -> OffsetDateTime {
    OffsetDateTime::from(time)
        .to_offset(UtcOffset::local_offset_at(OffsetDateTime::now_utc()).unwrap())
}

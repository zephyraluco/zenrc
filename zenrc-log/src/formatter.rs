use time::OffsetDateTime;
use time::macros::format_description;
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::{self, FormatEvent, FormatFields};
use tracing_subscriber::fmt::{FmtContext, FormattedFields};
use tracing_subscriber::registry::LookupSpan;

// 自定义日志格式化器
pub struct LogFormatter;

impl<S, N> FormatEvent<S, N> for LogFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(&self, ctx: &FmtContext<'_, S, N>, mut writer: format::Writer<'_>, event: &Event<'_>) -> std::fmt::Result {
        let metadata = event.metadata();

        // 打印时间戳
        let now = OffsetDateTime::now_local().expect("Failed to get local time");
        let format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]");
        write!(writer, "[{}] ", now.format(&format).expect("Failed to format time"))?;

        // 打印日志级别、target、文件和行号
        let file = metadata.file().unwrap_or("unknown");
        let line = metadata.line().map(|l| l.to_string()).unwrap_or_default();

        write!(&mut writer, "[{}] ", metadata.level())?;

        // 打印 span 信息
        if let Some(scope) = ctx.event_scope() {
            write!(writer, "[")?;
            let spans: Vec<_> = scope.from_root().collect();
            for (i, span) in spans.iter().enumerate() {
                write!(writer, "{}", span.name())?;

                let ext = span.extensions();
                if let Some(fields) = ext.get::<FormattedFields<N>>() {
                    if !fields.is_empty() {
                        write!(writer, "{{{}}}", fields)?;
                    }
                }

                // 除了最后一个 span，其余加 "/ "
                if i < spans.len() - 1 {
                    write!(writer, " / ")?;
                }
            }
            write!(writer, "] ")?;
        }
        // write!(writer, ": ")?;

        // 打印事件字段
        ctx.field_format().format_fields(writer.by_ref(), event)?;

        write!(writer, " [{}:{}]", file, line)?;
        writeln!(writer)
    }
}

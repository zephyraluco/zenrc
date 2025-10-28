use zenrc_log::formatter::LogFormatter;
use zenrc_log::*;

fn main() {
    let builder = SubscriberBuilder::new();
    builder
        .with_event_format(LogFormatter)
        .with_level(Level::INFO)
        .with_rotation(Period::Minute)
        .with_max_log_files(2)
        .with_path("./logs/app.log")
        .with_filter("sd", "sd.log")
        .init();
    // builder.init();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        info!(target : "sd","Hello, world! This is a log message.");
    }
}

use env_logger::{self, WriteStyle};
use log::LevelFilter;

/// Initializes the logging for the whole project;
pub fn init_logger() {
    let mut builder = env_logger::Builder::new();

    builder
        .filter(None, LevelFilter::Info)
        .write_style(WriteStyle::Always)
        .init();
}

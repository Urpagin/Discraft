use env_logger::{self, WriteStyle};
use log::LevelFilter;

/// Initializes the logging for the whole project;
pub fn init_logger() {
    let mut builder = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn"), // Default to warn
    );

    builder
        // Set the default logging level for all modules
        .filter(None, LevelFilter::Warn)
        // Suppress Serenity's detailed logs (set to Warn or higher)
        .filter_module("client", LevelFilter::Debug)
        .filter_module("server", LevelFilter::Debug)
        .filter_module("shared", LevelFilter::Debug)
        .write_style(WriteStyle::Always)
        .init();
}

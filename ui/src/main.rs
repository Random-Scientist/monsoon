use app::Monsoon;
use monsoon::MonsoonExt;

pub fn main() -> Result<(), iced::Error> {
    simple_logger::SimpleLogger::new()
        .env()
        .with_level(log::LevelFilter::Off)
        .with_module_level("app", log::LevelFilter::Trace)
        // .with_module_level("cargo_hot_protocol", log::LevelFilter::Trace)
        .init()
        .expect("no logger to be set");
    iced::daemon(Monsoon::init, Monsoon::update, Monsoon::view)
        .subscription(Monsoon::subscription)
        .title(Monsoon::title)
        .run()
}

use app::Monsoon;
use monsoon::MonsoonExt;

pub fn main() -> Result<(), iced::Error> {
    iced::daemon(Monsoon::init, Monsoon::update, Monsoon::view)
        .subscription(Monsoon::subscription)
        .title(Monsoon::title)
        .run()
}

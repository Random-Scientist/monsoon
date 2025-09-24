use ui::Monsoon;

fn main() -> iced::Result {
    iced::daemon(Monsoon::title, Monsoon::update, Monsoon::view)
        .subscription(Monsoon::subscription)
        .run_with(Monsoon::new)
}

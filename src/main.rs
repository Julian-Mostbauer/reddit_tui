mod reddit_api;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ui::run()
}

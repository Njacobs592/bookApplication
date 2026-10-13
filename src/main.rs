mod chapters;
mod documents;
mod features;
mod lore;
mod projects;
mod ui;

use gtk::prelude::*;

fn main() {
    let app = gtk::Application::builder()
        .application_id("com.njacobs.bookapp")
        .build();
    app.connect_activate(ui::build_ui);
    app.run();
}

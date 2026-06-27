// breadbin-gui - the GTK4/libadwaita desktop app.

mod app;
mod config;
mod demos;
mod kiosk;
mod task;
mod tunes;

use adw::prelude::*;

fn main() -> glib::ExitCode {
    breadbin_core::core::ensure_user_data_dir();
    let application = adw::Application::builder()
        .application_id(config::APP_ID)
        .build();
    application.connect_startup(|_| app::load_styles());
    application.connect_activate(app::build_ui);
    application.run()
}

// breadbin-gui - the GTK4/libadwaita desktop app. M1: window, view switcher (Games /
// Tunes / Demos), and the C64 skin. The views are populated in later milestones.

mod app;

use adw::prelude::*;

pub const APP_ID: &str = "io.github.jacobandresen.Breadbin";

fn main() -> glib::ExitCode {
    breadbin_core::core::ensure_user_data_dir();
    let application = adw::Application::builder().application_id(APP_ID).build();
    application.connect_startup(|_| app::load_styles());
    application.connect_activate(app::build_ui);
    application.run()
}

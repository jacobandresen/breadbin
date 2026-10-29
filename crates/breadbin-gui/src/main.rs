// breadbin-gui - the GTK4/libadwaita desktop app.

mod app;
mod cli;
mod config;
mod demos;
mod kiosk;
mod loading;
mod scene;
mod task;
mod tunes;
mod viz;

use adw::prelude::*;
use std::process::ExitCode;

fn main() -> ExitCode {
    breadbin_core::core::ensure_user_data_dir();
    
    // Check for CLI commands first
    if let Some(exit_code) = cli::handle_cli_args() {
        return exit_code;
    }
    
    // No CLI command, start the GUI
    let application = adw::Application::builder()
        .application_id(config::APP_ID)
        .build();
    application.connect_startup(|_| app::load_styles());
    application.connect_activate(app::build_ui);
    
    // Convert glib::ExitCode to std::process::ExitCode
    let glib_code = application.run();
    if glib_code == glib::ExitCode::SUCCESS {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

use gtk::gio;
use adw::prelude::*;

pub const APP_ID: &str = "io.github.jacobandresen.Breadbin";

pub struct Settings(gio::Settings);

impl Settings {
    pub fn new() -> Self {
        Self(gio::Settings::new(APP_ID))
    }

    pub fn forever(&self) -> Option<breadbin_core::roms::Forever> {
        let s = self.0.string("c64forever-roms");
        let configured: Option<String> = if s.is_empty() { None } else { Some(s.to_string()) };
        breadbin_core::roms::detect(configured.as_deref())
    }

    pub fn launch_opts(&self) -> Option<breadbin_core::run::LaunchOpts> {
        Some(breadbin_core::run::LaunchOpts {
            warp: self.0.boolean("warp"),
            fullscreen: self.0.boolean("fullscreen-launch"),
            keyboard: false,
            drive_sound: Some(self.0.boolean("drive-sound")),
            forever: self.forever()?,
        })
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

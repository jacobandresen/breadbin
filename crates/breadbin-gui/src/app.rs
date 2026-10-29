// app - top-level window: header bar, a Games/Tunes/Demos view switcher, and C64 skin.

use adw::prelude::*;
use gtk::gio;

use crate::config::Settings;

/// The Sixtyfour C64 pixel font (OFL), embedded so no install step is needed.
const SIXTYFOUR_TTF: &[u8] = include_bytes!("../data/fonts/Sixtyfour.ttf");

/// Register the embedded Sixtyfour font with the default font map. Pango loads
/// fonts by path, so it is written once into the data dir.
fn register_fonts() {
    let path = breadbin_core::core::data_path("fonts/Sixtyfour.ttf");
    let current = std::fs::metadata(&path).is_ok_and(|m| m.len() == SIXTYFOUR_TTF.len() as u64);
    if !current {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&path, SIXTYFOUR_TTF) {
            eprintln!("could not install Sixtyfour font: {e}");
            return;
        }
    }
    // GTK renders through the shared default font map; any widget's context reaches it.
    let Some(font_map) = gtk::Label::new(None).pango_context().font_map() else {
        return;
    };
    if let Err(e) = font_map.add_font_file(&path) {
        eprintln!("could not load Sixtyfour font: {e}");
    }
}

/// Load the C64 font and stylesheet into the default display.
pub fn load_styles() {
    register_fonts();
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("../data/style.css"));
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// The header's wordmark: "breadbin" beside the five Commodore stripes.
fn brand_badge() -> gtk::Box {
    let label = gtk::Label::new(Some("breadbin"));
    label.add_css_class("brand-label");

    let stripes = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(1)
        .valign(gtk::Align::Center)
        .build();
    for class in ["s1", "s2", "s3", "s4", "s5"] {
        let stripe = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        stripe.add_css_class("brand-stripe");
        stripe.add_css_class(class);
        stripes.append(&stripe);
    }

    let brand = gtk::Box::builder().spacing(8).valign(gtk::Align::Center).build();
    brand.add_css_class("brand");
    brand.append(&label);
    brand.append(&stripes);
    brand
}

pub fn build_ui(app: &adw::Application) {
    let settings = Settings::new();

    // Start-up screen over the whole window until the Games shelves are ready.
    let loader = crate::loading::Loader::new(
        &crate::kiosk::STARTUP_PHASES,
        crate::kiosk::first_startup_phase(),
    );
    let (kiosk_widget, kiosk_refresh) = crate::kiosk::build(&settings, Some(loader.sender()));

    let stack = adw::ViewStack::new();
    stack.add_titled_with_icon(
        &kiosk_widget,
        Some("kiosk"),
        "Games",
        "applications-games-symbolic",
    );
    stack.add_titled_with_icon(
        &crate::tunes::build(),
        Some("tunes"),
        "Tunes",
        "audio-x-generic-symbolic",
    );
    stack.add_titled_with_icon(
        &crate::demos::build(&settings),
        Some("demos"),
        "Demos",
        "video-display-symbolic",
    );
    
    // Add Scene/CSDb view
    let scene_widget = crate::scene::build_with_settings(&settings);
    stack.add_titled_with_icon(
        &scene_widget,
        Some("scene"),
        "Scene",
        "system-users-symbolic",
    );

    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();

    let menu = gio::Menu::new();
    menu.append(Some("Refresh catalogue"), Some("app.refresh"));
    menu.append(Some("Hidden games"), Some("app.hidden-games"));
    menu.append(Some("Sync CSDb data"), Some("app.sync-csdb"));
    menu.append(Some("About breadbin"), Some("app.about"));
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .build();

    let download_btn = gtk::Button::builder()
        .icon_name("folder-download-symbolic")
        .tooltip_text("Download all games and covers")
        .build();

    let header = adw::HeaderBar::builder().title_widget(&switcher).build();
    header.pack_start(&brand_badge());
    
    // Add device badges to the header bar if controllers are connected
    if let Some(device_badges) = crate::kiosk::create_device_badges() {
        header.pack_start(&device_badges);
    }
    
    header.pack_end(&menu_button);
    header.pack_end(&download_btn);

    let switcher_bar = adw::ViewSwitcherBar::builder().stack(&stack).build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&stack));
    toolbar.add_bottom_bar(&switcher_bar);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("breadbin")
        .default_width(1100)
        .default_height(740)
        .content(&{
            let overlay = gtk::Overlay::builder().child(&toolbar).build();
            overlay.add_overlay(loader.widget());
            overlay
        })
        .build();
    window.add_css_class("c64-font");

    // App actions
    let refresh_action = gio::SimpleAction::new("refresh", None);
    refresh_action.connect_activate(|_, _| {
        glib::spawn_future_local(async move {
            if let Err(e) = crate::task::run_blocking(|| breadbin_core::games::refresh(&mut |_| {})).await {
                eprintln!("catalogue refresh failed: {e}");
            }
        });
    });
    app.add_action(&refresh_action);

    let hidden_games_action = gio::SimpleAction::new("hidden-games", None);
    let win_weak_hg = window.downgrade();
    let kiosk_refresh_hg = kiosk_refresh.clone();
    hidden_games_action.connect_activate(move |_, _| {
        let refresh = kiosk_refresh_hg.clone();
        if let Some(w) = win_weak_hg.upgrade() {
            crate::kiosk::show_hidden_dialog(&w, refresh);
        }
    });
    app.add_action(&hidden_games_action);

    let sync_csdb_action = gio::SimpleAction::new("sync-csdb", None);
    sync_csdb_action.connect_activate(|_, _| {
        glib::spawn_future_local(async move {
            match crate::task::run_blocking(|| breadbin_core::csdb::sync_top_releases(100)).await {
                Ok(stats) => {
                    eprintln!("CSDb sync complete: {} releases fetched", stats.releases_fetched);
                    if !stats.errors.is_empty() {
                        eprintln!("Errors: {:?}", stats.errors);
                    }
                }
                Err(e) => {
                    eprintln!("CSDb sync failed: {}", e);
                }
            }
        });
    });
    app.add_action(&sync_csdb_action);

    let about_action = gio::SimpleAction::new("about", None);
    let win_weak = window.downgrade();
    about_action.connect_activate(move |_, _| {
        let dialog = adw::AboutDialog::builder()
            .application_name("breadbin")
            .version(env!("CARGO_PKG_VERSION"))
            .developer_name("Jacob Andresen")
            .build();
        if let Some(w) = win_weak.upgrade() {
            dialog.present(Some(&w));
        }
    });
    app.add_action(&about_action);

    window.present();

    let win_weak_dl = window.downgrade();
    let kiosk_refresh_dl = kiosk_refresh.clone();
    download_btn.connect_clicked(move |_| {
        if let Some(w) = win_weak_dl.upgrade() {
            crate::kiosk::show_download_all_dialog(&w, kiosk_refresh_dl.clone());
        }
    });
}

// app - top-level window: header bar, a Games/Tunes/Demos view switcher, and C64 skin.

use adw::prelude::*;
use gtk::gio;

use crate::config::Settings;

/// Load the C64 stylesheet into the default display.
pub fn load_styles() {
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

pub fn build_ui(app: &adw::Application) {
    let settings = Settings::new();

    let stack = adw::ViewStack::new();
    stack.add_titled_with_icon(
        &crate::kiosk::build(&settings),
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

    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();

    let menu = gio::Menu::new();
    menu.append(Some("Refresh catalogue"), Some("app.refresh"));
    menu.append(Some("About breadbin"), Some("app.about"));
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .build();

    let header = adw::HeaderBar::builder().title_widget(&switcher).build();
    header.pack_end(&menu_button);

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
        .content(&toolbar)
        .build();
    window.add_css_class("c64-font");

    // App actions
    let refresh_action = gio::SimpleAction::new("refresh", None);
    refresh_action.connect_activate(|_, _| {
        glib::spawn_future_local(async move {
            crate::task::run_blocking(|| breadbin_core::tui::refresh()).await.ok();
        });
    });
    app.add_action(&refresh_action);

    let about_action = gio::SimpleAction::new("about", None);
    let win_weak = window.downgrade();
    about_action.connect_activate(move |_, _| {
        let dialog = adw::AboutDialog::builder()
            .application_name("breadbin")
            .version("0.2.0")
            .developer_name("Jacob Andresen")
            .build();
        if let Some(w) = win_weak.upgrade() {
            dialog.present(Some(&w));
        }
    });
    app.add_action(&about_action);

    window.present();
}

// app - top-level window: header bar, a Games/Tunes/Demos view switcher, and the C64
// skin. Each page is a placeholder StatusPage for now; later milestones fill them in.

use adw::prelude::*;
use gtk::gio;

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

/// A placeholder page for a not-yet-built view.
fn placeholder(icon: &str, title: &str, body: &str) -> gtk::Widget {
    adw::StatusPage::builder()
        .icon_name(icon)
        .title(title)
        .description(body)
        .build()
        .upcast()
}

pub fn build_ui(app: &adw::Application) {
    let stack = adw::ViewStack::new();
    stack.add_titled_with_icon(
        &placeholder("applications-games-symbolic", "Games", "The cover library lands in M2."),
        Some("kiosk"),
        "Games",
        "applications-games-symbolic",
    );
    stack.add_titled_with_icon(
        &placeholder("audio-x-generic-symbolic", "Tunes", "The SID jukebox lands in M3."),
        Some("tunes"),
        "Tunes",
        "audio-x-generic-symbolic",
    );
    stack.add_titled_with_icon(
        &placeholder("video-display-symbolic", "Demos", "The demoscene browser lands in M3b."),
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
    window.present();
}

// breadbin-gui - the GTK4/libadwaita desktop app. M1+ builds the window here; for now
// this is a stub that ensures the data dir exists (the GUI is added in milestone M1).

fn main() {
    breadbin_core::core::ensure_user_data_dir();
    eprintln!("breadbin GUI not yet implemented (milestone M1). Core library is ready.");
}

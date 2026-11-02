//! serviceFlow entry point: a native window on desktop, a canvas in the browser (trunk).
//!
//! Desktop: `serviceflow [DATA_ROOT [SONG]]`. Without arguments it opens the data root chosen
//! last time, or asks for one on first run. `DATA_ROOT` is used for that run only. Set
//! `SERVICEFLOW_CONFIG_DIR=` (empty) to keep settings in memory only.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use serviceflow::data_root::DesktopApp;
    use serviceflow_core::command::Command;
    use serviceflow_core::ids::SongId;

    let mut args = std::env::args().skip(1);
    let root = args.next().map(std::path::PathBuf::from);
    let song = args.next();
    serviceflow::native::run(move |ctx, monitors| {
        let mut app = DesktopApp::new(ctx, craft_settings::Location::Auto, root).with_monitors(monitors);
        if let (Some(song), Some(window)) = (song, app.window()) {
            window.driver().send(Command::OpenSong(SongId::new(song)));
        }
        app
    })
}

#[cfg(target_arch = "wasm32")]
fn main() {
    serviceflow::web::start();
}

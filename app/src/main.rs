//! serviceFlow entry point: a native window on desktop, a canvas in the browser (trunk).
//!
//! Until filesystem storage lands (#4), the desktop build can preview a song from an OpenSong
//! data root copied into memory: `serviceflow [DATA_ROOT [SONG]]`.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use serviceflow_core::command::Command;
    use serviceflow_core::ids::SongId;
    use serviceflow_runtime::Driver;
    use serviceflow_storage_memory::MemoryStorage;

    let mut args = std::env::args().skip(1);
    let storage = match args.next() {
        Some(root) => MemoryStorage::from_data_root(std::path::Path::new(&root)).unwrap_or_else(|e| {
            eprintln!("serviceflow: cannot read data root {root}: {e}");
            std::process::exit(2);
        }),
        None => MemoryStorage::new(),
    };
    let mut driver = Driver::new(storage);
    if let Some(song) = args.next() {
        driver.send(Command::OpenSong(SongId::new(song)));
    }
    let options = eframe::NativeOptions { viewport: serviceflow::control_viewport(), ..Default::default() };
    eframe::run_native("serviceFlow", options, Box::new(|_cc| Ok(Box::new(serviceflow::ControlWindow::new(driver)))))
}

#[cfg(target_arch = "wasm32")]
fn main() {
    serviceflow::web::start();
}

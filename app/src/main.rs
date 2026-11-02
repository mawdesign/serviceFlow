//! serviceFlow entry point: a native window on desktop, a canvas in the browser (trunk).

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions { viewport: serviceflow::control_viewport(), ..Default::default() };
    eframe::run_native("serviceFlow", options, Box::new(|_cc| Ok(Box::new(serviceflow::ControlWindow::default()))))
}

#[cfg(target_arch = "wasm32")]
fn main() {
    serviceflow::web::start();
}

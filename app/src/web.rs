//! The browser build: runs the control window on the page's canvas.

use wasm_bindgen::JsCast as _;

/// Id of the `<canvas>` in `app/index.html`.
const CANVAS_ID: &str = "serviceflow_canvas";

pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().and_then(|w| w.document()).expect("no document");
        let canvas =
            document.get_element_by_id(CANVAS_ID).expect("no canvas").dyn_into::<web_sys::HtmlCanvasElement>().expect("not a canvas element");
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|_cc| {
                    Ok(Box::new(crate::ControlWindow::new(serviceflow_runtime::Driver::new(serviceflow_storage_memory::MemoryStorage::new()))))
                }),
            )
            .await;
        if let Some(loading) = document.get_element_by_id("serviceflow_loading") {
            match result {
                Ok(()) => loading.remove(),
                Err(e) => loading.set_text_content(Some(&format!("{} {e:?}", crate::tl!("serviceFlow could not start:")))),
            }
        }
    });
}

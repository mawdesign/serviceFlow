//! Desktop glue: run eframe on our own winit event loop so the monitors can be listed (eframe
//! exposes none), and keep the operator's preferences in craft-settings.
//!
//! This is the thin, untested layer: it needs a display. See `monitors` for the logic.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event_loop::{ActiveEventLoop, EventLoop};

use crate::monitors::{MonitorInfo, Monitors};

/// Forwards every callback to eframe's handler, and reads the monitor list on the way.
struct WithMonitors<'a> {
    inner: eframe::EframeWinitApplication<'a>,
    monitors: Monitors,
}

impl WithMonitors<'_> {
    fn refresh(&self, event_loop: &ActiveEventLoop) {
        let list = event_loop
            .available_monitors()
            .map(|m| MonitorInfo { name: m.name(), position: [m.position().x, m.position().y], size: [m.size().width, m.size().height] })
            .collect();
        if let Ok(mut shared) = self.monitors.lock() {
            *shared = list;
        }
    }
}

impl ApplicationHandler<eframe::UserEvent> for WithMonitors<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.refresh(event_loop);
        self.inner.resumed(event_loop);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: winit::window::WindowId, event: winit::event::WindowEvent) {
        // Monitors come and go, and scale changes mean the window moved to another one.
        if matches!(event, winit::event::WindowEvent::ScaleFactorChanged { .. } | winit::event::WindowEvent::Moved(_)) {
            self.refresh(event_loop);
        }
        self.inner.window_event(event_loop, id, event);
    }
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        self.inner.new_events(event_loop, cause);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: eframe::UserEvent) {
        self.inner.user_event(event_loop, event);
    }
    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: winit::event::DeviceId, event: winit::event::DeviceEvent) {
        self.inner.device_event(event_loop, id, event);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.about_to_wait(event_loop);
    }
    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.suspended(event_loop);
    }
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.exiting(event_loop);
    }
}

/// Run the desktop app until its window closes. `make` builds it once the egui context exists,
/// and receives the monitor list the event loop keeps up to date.
pub fn run<A: eframe::App + 'static>(make: impl FnOnce(&egui::Context, Monitors) -> A + 'static) -> Result<(), Box<dyn std::error::Error>> {
    let monitors = Monitors::default();
    let shared = Arc::clone(&monitors);

    let options = eframe::NativeOptions { viewport: crate::control_viewport(), ..Default::default() };
    let event_loop = EventLoop::<eframe::UserEvent>::with_user_event().build()?;
    let inner = eframe::create_native("serviceFlow", options, Box::new(move |cc| Ok(Box::new(make(&cc.egui_ctx, shared)))), &event_loop);
    let mut app = WithMonitors { inner, monitors };
    event_loop.run_app(&mut app)?;
    Ok(())
}

//! Monitors and the operator's choice of output monitor.
//!
//! eframe cannot list monitors, so the native `main` wraps its winit event loop to read them
//! (`native::run`) and shares the list with the control window. Indices are winit's, which is the
//! order `ViewportBuilder::with_monitor` uses. The browser has no monitor list.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// A monitor as winit reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorInfo {
    pub name: Option<String>,
    pub position: [i32; 2],
    pub size: [u32; 2],
}

impl MonitorInfo {
    /// For the monitor menu, such as `2: Monitor #7 (1920x1080)`.
    pub fn label(&self, index: usize) -> String {
        let [w, h] = self.size;
        match &self.name {
            Some(name) => format!("{}: {name} ({w}x{h})", index + 1),
            None => format!("{}: ({w}x{h})", index + 1),
        }
    }
}

/// The monitor list, filled in by the winit wrapper and read by the control window.
pub type Monitors = Arc<Mutex<Vec<MonitorInfo>>>;

/// The operator's remembered output monitor. Indices change when monitors are plugged in or out,
/// so the position and size (and the name, which macOS makes up) identify it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorChoice {
    pub name: Option<String>,
    pub position: [i32; 2],
    pub size: [u32; 2],
}

impl MonitorChoice {
    pub fn of(info: &MonitorInfo) -> Self {
        Self { name: info.name.clone(), position: info.position, size: info.size }
    }

    /// The index of the monitor this choice names: the same position and size, else the same
    /// name and size. `None` when it is not connected.
    pub fn resolve(&self, monitors: &[MonitorInfo]) -> Option<usize> {
        let same = |m: &MonitorInfo| m.size == self.size;
        monitors
            .iter()
            .position(|m| same(m) && m.position == self.position)
            .or_else(|| monitors.iter().position(|m| same(m) && self.name.is_some() && m.name == self.name))
    }
}

/// The monitor to use: the remembered choice if it is connected, else the second monitor (the
/// first is normally the one the control window is on), else the only one. `None` with no list.
pub fn pick(choice: Option<&MonitorChoice>, monitors: &[MonitorInfo]) -> Option<usize> {
    choice.and_then(|c| c.resolve(monitors)).or(match monitors.len() {
        0 => None,
        1 => Some(0),
        _ => Some(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: &str, x: i32, w: u32) -> MonitorInfo {
        MonitorInfo { name: Some(name.into()), position: [x, 0], size: [w, 1080] }
    }

    #[test]
    fn a_remembered_monitor_is_found_again_even_when_the_order_changes() {
        let projector = monitor("Monitor #9", 1920, 1280);
        let choice = MonitorChoice::of(&projector);
        assert_eq!(choice.resolve(&[monitor("Monitor #1", 0, 1920), projector.clone()]), Some(1));
        assert_eq!(choice.resolve(&[projector, monitor("Monitor #1", 0, 1920)]), Some(0));
    }

    #[test]
    fn a_monitor_that_moved_is_found_by_name() {
        let choice = MonitorChoice::of(&monitor("Monitor #9", 1920, 1280));
        assert_eq!(choice.resolve(&[monitor("Monitor #1", 0, 1920), monitor("Monitor #9", 3840, 1280)]), Some(1));
    }

    #[test]
    fn a_disconnected_monitor_falls_back_to_the_second_then_the_only_one() {
        let gone = MonitorChoice::of(&monitor("Gone", 5000, 800));
        let two = [monitor("A", 0, 1920), monitor("B", 1920, 1280)];
        assert_eq!(pick(Some(&gone), &two), Some(1));
        assert_eq!(pick(None, &two), Some(1));
        assert_eq!(pick(None, &two[..1]), Some(0));
        assert_eq!(pick(None, &[]), None);
    }

    #[test]
    fn the_menu_label_is_numbered_from_one() {
        assert_eq!(monitor("Monitor #9", 0, 1280).label(1), "2: Monitor #9 (1280x1080)");
    }
}

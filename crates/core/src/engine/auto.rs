//! Auto-advance: the slides of a custom or image item moving on by themselves (GLOSSARY
//! "Auto-advance"). The engine has no clock; whoever drives it sends [`Command::Tick`] with the
//! time, and the state says when the next advance is due, so a fake clock tests all of it.

use std::time::Duration;

use super::{go_live, live_index};
use crate::state::{OutputMode, State};

/// Decide, after what is shown changed, whether the live slide advances by itself and when.
/// Anything but the plain live slide (blank, black, logo, the blank between items) stops it.
pub(super) fn schedule(state: &mut State) {
    state.output.auto_due = due(state);
}

fn due(state: &State) -> Option<Duration> {
    if state.output.mode != OutputMode::Normal {
        return None;
    }
    let advance = state.output.live.as_ref()?.advance;
    if advance.seconds == 0 {
        return None;
    }
    // Only the item that is open (not one the operator has browsed away to) advances, and only
    // while there is a slide to go to.
    let index = live_index(state)?;
    let in_flow = state.song.as_ref()?.in_flow;
    let moves = index < in_flow && (index + 1 < in_flow || (advance.looping && in_flow > 1));
    moves.then(|| state.now + Duration::from_secs(u64::from(advance.seconds)))
}

/// It is `now`: advance as often as the time passed says (a long pause catches up).
pub(super) fn tick(state: &mut State, now: Duration) {
    // A cap, so a clock jumping by days cannot spin.
    for _ in 0..1000 {
        let Some(due) = state.output.auto_due.filter(|due| *due <= now) else { break };
        state.now = due;
        state.output.auto_due = None;
        fire(state);
    }
    state.now = now;
}

/// One automatic step: the next slide, or the first after the last when looping.
fn fire(state: &mut State) {
    let Some(live) = &state.output.live else { return };
    let looping = live.advance.looping;
    let Some(index) = live_index(state) else { return };
    let in_flow = state.song.as_ref().map_or(0, |open| open.in_flow);
    if index + 1 < in_flow {
        go_live(state, index + 1);
    } else if looping && in_flow > 1 {
        go_live(state, 0);
    }
}

//! The engine: the single writer of [`State`] (ADR-0001).

use std::sync::Arc;

use crate::command::{Command, Effect};
use crate::state::{LiveSlide, OpenSong, OutputMode, Problem, State};

#[derive(Debug, Default)]
pub struct Engine {
    state: Arc<State>,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current state. A snapshot never changes; applying a command publishes a new one.
    pub fn snapshot(&self) -> Arc<State> {
        Arc::clone(&self.state)
    }

    /// Apply one command and return the IO it asks for.
    pub fn apply(&mut self, command: Command) -> Vec<Effect> {
        // Copy-on-write: snapshots handed out earlier keep the old state.
        let state = Arc::make_mut(&mut self.state);
        match command {
            Command::OpenSong(id) => {
                state.loading = Some(id.clone());
                vec![Effect::LoadSong(id)]
            }
            Command::SongLoaded { id, song, hash } => {
                // A result for a song that is no longer wanted (another was opened since) is dropped.
                if state.loading.as_ref() == Some(&id) {
                    state.loading = None;
                    let slides = song.slides();
                    state.song = Some(OpenSong { id, song, hash, slides });
                }
                Vec::new()
            }
            Command::SongLoadFailed { id, error } => {
                if state.loading.as_ref() == Some(&id) {
                    state.loading = None;
                }
                state.problems.push(Problem::SongUnreadable { id, error });
                Vec::new()
            }
            Command::GoLive { index } => {
                go_live(state, index);
                Vec::new()
            }
            Command::NextSlide => {
                let target = match live_index(state) {
                    Some(i) => i + 1,
                    None => 0,
                };
                go_live(state, target);
                Vec::new()
            }
            Command::PreviousSlide => {
                if let Some(i) = live_index(state).and_then(|i| i.checked_sub(1)) {
                    go_live(state, i);
                }
                Vec::new()
            }
            Command::ToggleBlank => {
                toggle(state, OutputMode::Blank);
                Vec::new()
            }
            Command::ToggleBlack => {
                toggle(state, OutputMode::Black);
                Vec::new()
            }
        }
    }
}

/// The live slide's index, when it belongs to the open song.
fn live_index(state: &State) -> Option<usize> {
    let live = state.output.live.as_ref()?;
    (state.song.as_ref()?.id == live.song).then_some(live.index)
}

/// Make slide `index` of the open song live and leave blank or black mode. Out of range: no change.
fn go_live(state: &mut State, index: usize) {
    let Some(open) = &state.song else { return };
    let Some(slide) = open.slides.get(index) else { return };
    state.output.live = Some(LiveSlide { song: open.id.clone(), index, slide: slide.clone() });
    state.output.mode = OutputMode::Normal;
}

/// Switch `mode` on, or back to normal when it is already on. Blank and black replace each other.
fn toggle(state: &mut State, mode: OutputMode) {
    state.output.mode = if state.output.mode == mode { OutputMode::Normal } else { mode };
}

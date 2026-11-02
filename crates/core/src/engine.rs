//! The engine: the single writer of [`State`] (ADR-0001).

use std::sync::Arc;

use crate::command::{Command, Effect};
use crate::state::{OpenSong, Problem, State};

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
            Command::SongLoaded { id, song } => {
                // A result for a song that is no longer wanted (another was opened since) is dropped.
                if state.loading.as_ref() == Some(&id) {
                    state.loading = None;
                    let slides = song.slides();
                    state.song = Some(OpenSong { id, song, slides });
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
        }
    }
}

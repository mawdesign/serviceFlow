//! The runtime: owns the [`Engine`] and a storage, performs the engine's [`Effect`]s, and feeds
//! their outcomes back in as commands (ADR-0001, ADR-0004).
//!
//! It is executor-agnostic and single-threaded: effects run as local futures on a
//! [`LocalPool`] that [`Driver::pump`] advances without blocking. The app pumps once per frame;
//! tests pump until idle. Nothing here blocks, so the same code runs in the browser.

use std::rc::Rc;
use std::sync::Arc;

use futures::channel::mpsc;
use futures::executor::{LocalPool, LocalSpawner};
use futures::task::LocalSpawnExt as _;
use serviceflow_core::command::{Command, Effect};
use serviceflow_core::engine::Engine;
use serviceflow_core::state::State;
use serviceflow_storage::Storage;

/// Runs the engine against a storage.
pub struct Driver<S> {
    engine: Engine,
    storage: Rc<S>,
    pool: LocalPool,
    spawner: LocalSpawner,
    results: mpsc::UnboundedReceiver<Command>,
    results_sender: mpsc::UnboundedSender<Command>,
    in_flight: usize,
}

impl<S: Storage + 'static> Driver<S> {
    pub fn new(storage: S) -> Self {
        let pool = LocalPool::new();
        let spawner = pool.spawner();
        let (results_sender, results) = mpsc::unbounded();
        Self { engine: Engine::new(), storage: Rc::new(storage), pool, spawner, results, results_sender, in_flight: 0 }
    }

    /// The current state snapshot.
    pub fn snapshot(&self) -> Arc<State> {
        self.engine.snapshot()
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    /// Apply a command now and start the IO it asks for. Results arrive on later [`pump`]s.
    ///
    /// [`pump`]: Driver::pump
    pub fn send(&mut self, command: Command) {
        for effect in self.engine.apply(command) {
            self.start(effect);
        }
    }

    /// Advance in-flight effects as far as they can go without waiting, applying every result
    /// command (and starting any effects those ask for). Never blocks.
    pub fn pump(&mut self) {
        loop {
            self.pool.run_until_stalled();
            let mut progressed = false;
            while let Ok(command) = self.results.try_recv() {
                self.in_flight -= 1;
                progressed = true;
                self.send(command);
            }
            if !progressed {
                break;
            }
        }
    }

    /// `true` when no effect is still waiting on storage.
    pub fn is_idle(&self) -> bool {
        self.in_flight == 0
    }

    fn start(&mut self, effect: Effect) {
        let storage = Rc::clone(&self.storage);
        let sender = self.results_sender.clone();
        self.in_flight += 1;
        let task = async move {
            let command = perform(&*storage, effect).await;
            // The receiver lives as long as the driver, which owns the pool running this task.
            let _ = sender.unbounded_send(command);
        };
        if self.spawner.spawn_local(task).is_err() {
            // Only possible once the pool is gone, which cannot happen while `self` exists.
            self.in_flight -= 1;
        }
    }
}

/// Perform one effect and return the command that reports its outcome.
pub async fn perform<S: Storage>(storage: &S, effect: Effect) -> Command {
    match effect {
        Effect::LoadSong(id) => {
            let parsed = match storage.read_song(&id).await {
                Ok(stored) => serviceflow_opensong::read_song(&stored.bytes).map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            match parsed {
                Ok(song) => Command::SongLoaded { id, song: Arc::new(song) },
                Err(error) => Command::SongLoadFailed { id, error },
            }
        }
    }
}

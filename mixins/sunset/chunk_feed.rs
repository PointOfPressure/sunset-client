// The chunks a module looks at on its own thread: each newly loaded chunk is sent there once (again if it unloads
// and loads), and what the thread made of it comes back. Dropping the feed ends the thread.

use super::Game;
use horizon_client::chunk::Chunk;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

pub(super) struct Feed<T> {
    // What the thread was started with (a dimension, settings): another key means another feed.
    pub(super) key: String,
    revision: u64,
    sent: HashSet<(i32, i32)>,
    jobs: Sender<Arc<Chunk>>,
    found: Receiver<T>,
}

impl<T: Send + 'static> Feed<T> {
    // A thread named `name` that runs `look` on each chunk, which puts what it finds in the Vec it is given.
    pub(super) fn start(name: &str, key: String, mut look: impl FnMut(&Chunk, &mut Vec<T>) + Send + 'static) -> Option<Self> {
        let (jobs, rx) = channel::<Arc<Chunk>>();
        let (tx, found) = channel();
        let thread = move || {
            let mut out = Vec::new();
            for chunk in rx {
                look(&chunk, &mut out);
                for t in out.drain(..) {
                    if tx.send(t).is_err() {
                        return;
                    }
                }
            }
        };
        std::thread::Builder::new().name(format!("sunset-{name}")).spawn(thread).ok()?;
        Some(Self { key, revision: 0, sent: HashSet::new(), jobs, found })
    }

    // Newly loaded chunks to the thread (unloaded ones forgotten), and what it found since last time.
    pub(super) fn pump(&mut self, game: &Game<'_>) -> Vec<T> {
        let revision = game.world.chunks.membership_revision();
        if self.revision != revision {
            self.revision = revision;
            self.sent.retain(|c| game.world.chunks.contains_key(c));
            for (c, chunk) in game.world.chunks.iter() {
                if self.sent.insert(*c) {
                    let _ = self.jobs.send(chunk.clone());
                }
            }
        }
        self.found.try_iter().collect()
    }
}

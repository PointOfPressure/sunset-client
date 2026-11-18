// The chat screen's completion for a line that is not a `/command`: the game (the binary) gives this
// library the function that answers it, since the library cannot call into the game.
use std::sync::OnceLock;

/// The text up to the cursor in, where the word to complete starts (a byte index) and the candidates out.
pub type Provider = fn(&str) -> Option<(usize, Vec<String>)>;

static PROVIDER: OnceLock<Provider> = OnceLock::new();

pub fn set(provider: Provider) {
    let _ = PROVIDER.set(provider);
}

pub fn get() -> Option<Provider> {
    PROVIDER.get().copied()
}

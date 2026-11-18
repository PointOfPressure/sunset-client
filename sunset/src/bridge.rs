// Calls from Sunset's hooks below the game crate (renderer, client library, protocol) into its game-side code.
//
// A hook in a crate below the game is compiled into that crate's own patch library, which can call this crate but
// not Sunset's modules in the game crate. Those modules register their functions here (as the game starts its first
// frame); a hook calls the one registered, passing its values as untyped pointers that the game-side function turns
// back into the types it was written for. A pointer is used only during the call. Before a function is registered
// a hook does nothing, as Sunset's own code would not have run yet.
use std::sync::OnceLock;

pub struct Hook<F: Copy>(OnceLock<F>);

impl<F: Copy> Hook<F> {
    pub const fn new() -> Self {
        Self(OnceLock::new())
    }
    pub fn set(&self, f: F) {
        let _ = self.0.set(f);
    }
    pub fn get(&self) -> Option<F> {
        self.0.get().copied()
    }
}

impl<F: Copy> Default for Hook<F> {
    fn default() -> Self {
        Self::new()
    }
}

pub type Ptr = *mut u8;

// A configuration custom payload (horizon_client::net::receive_brand): &CustomPayload.
pub static CONFIG_PAYLOAD: Hook<fn(Ptr)> = Hook::new();
// The renderer's frame (horizon_client::gfx::Renderer::render): &mut Renderer, &Frame.
pub static RENDER: Hook<fn(Ptr, Ptr)> = Hook::new();
// The renderer giving up its surface (Renderer::release_surface): &mut Renderer.
pub static RELEASE_SURFACE: Hook<fn(Ptr)> = Hook::new();

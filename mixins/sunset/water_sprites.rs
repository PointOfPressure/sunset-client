// Clear Water: a water sprite is laid out at every lower opacity just before itself, so its own layer follows them.
fn sunset_water_sprites(this: &mut Sprites, name: &str, decoded: &crate::sprite_cache::Decoded) -> std::ops::ControlFlow<Result<u32>> {
    use crate::mesher::sunset_water as w;
    let Some(sprite) = w::SPRITES.iter().position(|s| *s == name) else {
        return std::ops::ControlFlow::Continue(());
    };
    let path = name.trim_start_matches("minecraft:");
    let mut copies = [0; w::STEPS];
    for (step, layer) in copies.iter_mut().enumerate() {
        let mut copy = decoded.clone();
        copy.frames = w::faded(&decoded.frames, step);
        copy.opaque = false;
        copy.has_transparent |= step == 0;
        copy.translucent = step > 0;
        match this.register(&["sunset:", path, "_", &step.to_string()].concat(), copy) {
            Ok(l) => *layer = l,
            Err(e) => return std::ops::ControlFlow::Break(Err(e)),
        }
    }
    w::set_layers(sprite, this.native.len() as u32, copies);
    std::ops::ControlFlow::Continue(())
}

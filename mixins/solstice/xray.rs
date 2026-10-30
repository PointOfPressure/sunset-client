// Xray: while a section mesh is built, only the listed blocks are there.
fn solstice_xray_hook(this: &mut Mesher<'_>, region: &[[Option<&Chunk>; 3]; 3], sy: i32) -> std::ops::ControlFlow<SectionMesh> {
    use std::sync::atomic::Ordering::Relaxed;
    if !solstice_xray::ON.load(Relaxed) || solstice_xray::meshing() {
        return std::ops::ControlFlow::Continue(());
    }
    solstice_xray::set_meshing(true);
    let mesh = this.mesh(region, sy);
    solstice_xray::set_meshing(false);
    std::ops::ControlFlow::Break(mesh)
}

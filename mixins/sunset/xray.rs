// Xray: while a section mesh is built, only the listed blocks are there. Clear Water: the built mesh's water is
// drawn fainter and lighter.
fn sunset_xray_hook(this: &mut Mesher<'_>, region: &[[Option<&Chunk>; 3]; 3], sy: i32) -> std::ops::ControlFlow<SectionMesh> {
    use std::sync::atomic::Ordering::Relaxed;
    let (xray, water) = (sunset_xray::ON.load(Relaxed), sunset_water::on());
    if !(xray || water) || sunset_xray::meshing() {
        return std::ops::ControlFlow::Continue(());
    }
    if let Some(c) = region[1][1].filter(|_| xray) {
        sunset_xray::set_origin([c.x * 16, c.min_y() + sy * 16, c.z * 16]);
    }
    sunset_xray::set_meshing(true);
    let mut mesh = this.mesh(region, sy);
    sunset_xray::set_meshing(false);
    if water {
        sunset_water::clear(&mut mesh);
    }
    std::ops::ControlFlow::Break(mesh)
}

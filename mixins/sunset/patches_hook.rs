// Sunset's glyphs: the coverage goes in the alpha over white, which an ordinary blend wants, instead of grey in
// every channel (which blends as the coverage squared and thins every edge), and a touch of weight is added.
// Only grey texels are changed, so the bitmap fonts' white and transparent ones stay as they are.
fn sunset_patches(this: &Font) -> std::ops::ControlFlow<Vec<Patch>> {
    if sunset_font::STYLE.load(std::sync::atomic::Ordering::Relaxed) == 0 {
        return std::ops::ControlFlow::Continue(());
    }
    let mut patches = std::mem::take(&mut this.cache.borrow_mut().region.pending);
    for patch in &mut patches {
        for px in patch.rgba.chunks_exact_mut(4) {
            if px[0] == px[3] && px[1] == px[3] && px[2] == px[3] {
                let coverage = f32::from(px[3]) / 255.0;
                px.copy_from_slice(&[255, 255, 255, (coverage.powf(0.8) * 255.0).round() as u8]);
            }
        }
    }
    std::ops::ControlFlow::Break(patches)
}

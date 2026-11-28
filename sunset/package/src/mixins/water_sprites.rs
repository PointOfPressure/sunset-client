// Clear Water's lighter tint: each biome's water colour, as a mod's biome_colors answer gives it.
fn sunset_water_tint(colors: &mut BiomeColors) {
    if ::sunset_core::lifecycle::RUNTIME.running() {
        colors.water = ::sunset_core::water::lighter(colors.water);
    }
}

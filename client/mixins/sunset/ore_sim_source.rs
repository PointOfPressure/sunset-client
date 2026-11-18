// Sunset's Ore Sim, the part that needs the bundle's own world presets: the decoration of a preset's dimension
// (built once and cached by the bundle, as a world opening builds it), handed to the replay.

pub use super::super::generator_decoration::sunset_ore_sim_replay::{KINDS, OreSim, SeenWorld};
use super::GeneratorData;
use std::sync::Arc;

// The ore roots of `preset`'s `dimension`, ready to replay for any seed: `build` is the level's build range
// (minimum, height), `generation` the generator's (minimum, height, sea level). None when the bundle has no
// features for it.
pub fn ore_sim(
    data: &Arc<GeneratorData>,
    preset: &str,
    dimension: &str,
    build: (i32, i32),
    generation: (i32, i32, i32),
) -> Result<Option<OreSim>, String> {
    let preset_json: serde_json::Value =
        serde_json::from_str(data.world_presets.get(preset).ok_or("no such world preset")?).map_err(|e| e.to_string())?;
    let source_json = preset_json["dimensions"][dimension]["generator"]["biome_source"].to_string();
    let source = data
        .source_catalog
        .decode(&source_json)
        .map_err(|e| e.to_string())?
        .resolve(&data.presets)
        .map_err(|e| e.to_string())?;
    let decoration = data.decoration_for(&source_json, &source).map_err(|e| e.to_string())?;
    let context = super::super::anchor::WorldGenerationContext::new(generation.0, generation.1, generation.2);
    Ok(decoration.map(|d| OreSim::new(d, data.registry(), data.biome_names(), build, context)))
}

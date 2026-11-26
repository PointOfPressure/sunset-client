// Atmosphere's Custom fog. The game clears the distance fog here when its Fog option is off; Sunset turns the option
// off while Custom fog is on, and sets the fog's distances and colour here instead (in air: water and lava keep
// theirs).
fn sunset_ambience_fog(fog: &mut FogData, input: &FogInputs, operation: impl FnOnce(&mut FogData, &FogInputs) -> ()) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return operation(fog, input); }

    let custom = *::sunset_core::fog::CUSTOM.lock().unwrap_or_else(|e| e.into_inner());
    let Some((start, end, rgb, cover)) = custom.filter(|_| input.fog_type == FogType::Atmospheric) else {
        return operation(fog, input);
    };
    (fog.environmental_start, fog.environmental_end) = (start, end);
    (fog.render_distance_start, fog.render_distance_end) = (start, end);
    fog.color = [rgb[0], rgb[1], rgb[2], fog.color[3]];
    if cover {
        fog.sky_end = end;
    }
}

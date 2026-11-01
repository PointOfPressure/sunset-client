// Atmosphere's sky, made in the shader alone (no textures, no ray marching, nothing kept from frame to frame).
// The renderer appends this to its existing Globals/sky declarations. Quality specializes
// a bounded 2/4/6-octave cloud field and 1/2/3 star layers; all modes use one cached pipeline.
pub const ATMOSPHERE_WGSL: &str = r#"
override SUNSET_MODE: u32 = 0u;
override SUNSET_BRIGHTNESS: f32 = 1.0;
override SUNSET_STRENGTH: f32 = 1.0;
override SUNSET_STARS: f32 = 1.0;
override SUNSET_SPEED: f32 = 1.0;
override SUNSET_DAY_BLEND: f32 = 1.0;
override SUNSET_QUALITY: u32 = 1u;

// Integer mixing keeps a world cell's seed independent of camera and animation time.
fn sunset_hash(cell: vec3<i32>) -> u32 {
    let p = bitcast<vec3<u32>>(cell);
    var h = p.x * 1597334677u ^ p.y * 3812015801u ^ p.z * 2798796415u;
    h = (h ^ (h >> 16u)) * 2246822519u;
    h = (h ^ (h >> 13u)) * 3266489917u;
    return h ^ (h >> 16u);
}
fn sunset_random(cell: vec3<i32>) -> f32 {
    return f32(sunset_hash(cell) >> 8u) * (1.0 / 16777216.0);
}
fn sunset_noise(p: vec3<f32>) -> f32 {
    let cell = vec3<i32>(floor(p));
    let f = fract(p);
    let s = f * f * (3.0 - 2.0 * f);
    let a = mix(sunset_random(cell), sunset_random(cell + vec3(1, 0, 0)), s.x);
    let b = mix(sunset_random(cell + vec3(0, 1, 0)), sunset_random(cell + vec3(1, 1, 0)), s.x);
    let c = mix(sunset_random(cell + vec3(0, 0, 1)), sunset_random(cell + vec3(1, 0, 1)), s.x);
    let d = mix(sunset_random(cell + vec3(0, 1, 1)), sunset_random(cell + vec3(1, 1, 1)), s.x);
    return mix(mix(a, b, s.y), mix(c, d, s.y), s.z);
}
fn sunset_cloud(position: vec3<f32>) -> vec2<f32> {
    var p = position;
    var sum = vec2(0.0);
    var amplitude = 0.5;
    var weight = 0.0;
    let octaves = 2u + min(SUNSET_QUALITY, 2u) * 2u;
    // The literal cap is part of the performance contract, even if specialization fails.
    for (var i = 0u; i < 6u; i++) {
        if i >= octaves { break; }
        let n = sunset_noise(p);
        sum += vec2(n, 1.0 - abs(n * 2.0 - 1.0)) * amplitude;
        weight += amplitude;
        // Rotate the domain between octaves to avoid axis-aligned cloud structure.
        p = vec3(p.y + p.z * 0.4, p.z - p.x * 0.4, p.x + p.y * 0.4) * 1.91 + vec3(7.1, 13.7, 3.9);
        amplitude *= 0.49;
    }
    return sum / weight;
}

// A thin spherical slice through a seeded 3-D lattice: no longitude seam or polar pinching.
// Seeds stay inside their cells with enough margin that the star's support never clips.
fn sunset_star_layer(ray: vec3<f32>, scale: f32, seed: i32, footprint: f32, time: f32) -> vec3<f32> {
    let p = ray * scale;
    let cell = vec3<i32>(floor(p)) + vec3(seed);
    let h = sunset_hash(cell);
    let center = vec3(
        f32(h & 1023u), f32((h >> 10u) & 1023u), f32((h >> 20u) & 1023u)
    ) * (0.64 / 1023.0) + vec3(0.18);
    let delta = fract(p) - center;
    let distance_to_star = length(delta);
    let rare = f32(h >> 30u) / 3.0;
    let radius = mix(0.055, 0.105, rare * rare * rare);
    let pixel = max(footprint * scale, 0.0001);
    let core = 1.0 - smoothstep(max(radius - pixel, 0.0), radius + pixel, distance_to_star);
    // Keep subpixel stars from becoming overbright as their filtered footprint grows.
    let coverage = min(1.0, radius * radius / (pixel * pixel));
    let halo = max(0.0, 1.0 - distance_to_star / 0.16);
    let twinkle = 0.88 + 0.12 * sin(time * 0.37 + f32(h & 255u));
    let tint = mix(vec3(0.58, 0.73, 1.0), vec3(1.0, 0.82, 0.71), f32(h & 255u) / 255.0);
    return tint * ((core * coverage * (0.65 + rare * 1.4)) + halo * halo * 0.035) * twinkle;
}

fn sunset_space(ray: vec3<f32>, time: f32) -> vec3<f32> {
    // Pre-normalized constants also avoid a Naga 30 override-folding edge case.
    let normal = vec3(0.511383, 0.641735, -0.571546);
    let across = vec3(-0.745241, 0.0, -0.666795);
    let along = vec3(-0.427905, 0.766926, 0.478247);
    let p = vec3(dot(ray, across), dot(ray, along), dot(ray, normal));
    let drift = vec3(time * 0.0008, -time * 0.0003, 0.0);
    let warp = vec2(sunset_noise(p * 2.6 + drift + vec3(4.7)),
                    sunset_noise(p * 2.6 + drift + vec3(19.3, 2.1, 8.4)));
    let field = sunset_cloud(p * vec3(4.2, 4.2, 6.4) + vec3(warp * 1.7, 0.0) + drift);
    let latitude = p.z + (warp.x - 0.5) * 0.31;
    let envelope = exp(-latitude * latitude * 5.2);
    let cloud = smoothstep(0.27, 0.76, field.x) * envelope;
    let filament = pow(clamp(field.y, 0.0, 1.0), 3.0) * cloud;
    let pink = smoothstep(0.28, 0.72, warp.y + field.x * 0.18);
    let tint = mix(vec3(0.035, 0.12, 0.62), vec3(0.75, 0.025, 0.42), pink);
    let dust = exp(-abs(latitude + (field.x - 0.5) * 0.35) * 29.0)
        * smoothstep(0.26, 0.64, warp.y);
    let emission = (tint * cloud * 1.45 + vec3(0.22, 0.16, 0.37) * filament * filament)
        * (1.0 - dust * 0.86);
    return vec3(0.002, 0.003, 0.012) + vec3(0.006, 0.009, 0.032) * envelope
        + emission * clamp(SUNSET_STRENGTH, 0.0, 2.0);
}

fn sunset_aurora(ray: vec3<f32>, time: f32, footprint: f32) -> vec3<f32> {
    let azimuth = atan2(ray.z, ray.x);
    var glow = vec3(0.0);
    // Three analytic curtains, not an unbounded volumetric ray march. All azimuth
    // frequencies are integers so atan2's branch cut cannot form a visible seam.
    for (var i = 0u; i < 3u; i++) {
        let layer = f32(i);
        let phase = time * 0.035 + layer * 2.1;
        let folds = sin(azimuth * 9.0 + phase + sin(azimuth * 3.0 - phase) * 1.3);
        let base = 0.12 + layer * 0.12 + sin(azimuth * 2.0 + phase) * 0.13 + folds * 0.035;
        let height = ray.y - base;
        let rise = smoothstep(-footprint * 2.0, 0.027 + footprint * 2.0, height);
        let fall = exp(-max(height, 0.0) * (7.0 + layer));
        let rays = 0.35 + 0.65 * pow(0.5 + 0.5 * sin(azimuth * 113.0 + folds * 2.0 + phase), 3.0);
        let curtain = rise * fall * rays;
        let tint = mix(vec3(0.025, 0.65, 0.32), vec3(0.29, 0.055, 0.58),
            smoothstep(0.02, 0.31, height));
        glow += tint * curtain * (0.7 - layer * 0.15);
    }
    return vec3(0.002, 0.007, 0.021) + glow * clamp(SUNSET_STRENGTH, 0.0, 2.0);
}

fn sunset_ringed_planet(ray: vec3<f32>, background: vec3<f32>, footprint: f32, time: f32) -> vec3<f32> {
    // Radius one at distance six: 9.59 degrees apparent radius, world anchored.
    // All fixed directions are pre-normalized; no constant normalize folding.
    let planet_center = vec3(3.0, 3.0, 4.2426407);
    let axis = vec3(0.15003751, 0.97024259, -0.19004752);
    let light = vec3(-0.63142230, 0.35079017, -0.69155776);
    let projection = dot(ray, planet_center);
    let discriminant = projection * projection - 35.0;
    let impact = sqrt(max(36.0 - projection * projection, 0.0));
    let pixel = max(footprint * 6.0, 0.0001);
    let sphere_mask = (1.0 - smoothstep(1.0 - pixel, 1.0 + pixel, impact))
        * select(0.0, 1.0, projection > 0.0);
    let sphere_t = projection - sqrt(max(discriminant, 0.0));
    var planet = vec3(0.0);
    if sphere_mask > 0.0 {
        let hit = ray * sphere_t - planet_center;
        let normal = hit / max(length(hit), 0.0001);
        let latitude = dot(normal, axis);
        let flow = sin(latitude * 43.0 + sin(normal.x * 9.0 + normal.z * 5.0) * 0.65 + time * 0.002);
        let bands = 0.5 + 0.5 * flow;
        let pigment = mix(vec3(0.23, 0.16, 0.34), vec3(0.88, 0.62, 0.43), bands * 0.65 + 0.22);
        let daylight = max(dot(normal, light), 0.0);
        planet = pigment * (0.10 + daylight * 1.10);
        let rim = pow(1.0 - max(dot(normal, -ray), 0.0), 3.0);
        planet += vec3(0.12, 0.18, 0.39) * rim * (0.2 + daylight);
    }
    var ring_alpha = 0.0;
    var ring_color = vec3(0.0);
    var ring_t = 100.0;
    let denominator = dot(ray, axis);
    // The forward, finite interval excludes near-parallel intersections before
    // division can magnify their precision error into an unbounded ring radius.
    if abs(denominator) > 0.0001 {
        let t = 2.5545370 / denominator;
        if t > 0.0 && t < 12.0 {
            let relative = ray * t - planet_center;
            let radius = length(relative);
            let ring_pixel = clamp(footprint * t / max(abs(denominator), 0.08), 0.0001, 0.12);
            let annulus = smoothstep(1.23 - ring_pixel, 1.28 + ring_pixel, radius)
                * (1.0 - smoothstep(2.05 - ring_pixel, 2.10 + ring_pixel, radius));
            let gap = smoothstep(1.69 - ring_pixel, 1.73 + ring_pixel, radius)
                * (1.0 - smoothstep(1.79 - ring_pixel, 1.83 + ring_pixel, radius));
            let fine = 1.0 - smoothstep(0.008, 0.035, ring_pixel);
            let grain = 0.67 + sin(radius * 73.0) * 0.15
                + sin(radius * 211.0) * 0.08 * fine;
            let pigment = mix(vec3(0.34, 0.24, 0.47), vec3(0.95, 0.72, 0.60), grain);
            // Test the ray from this ring point toward the light against the sphere.
            let toward_light = dot(relative, light);
            let shadow_disc = toward_light * toward_light - (dot(relative, relative) - 1.0);
            let shadow = select(1.0, 0.16, toward_light < 0.0 && shadow_disc > 0.0);
            ring_color = pigment * shadow * 0.85;
            ring_alpha = annulus * (1.0 - gap * 0.88) * (0.65 + grain * 0.30);
            ring_t = t;
        }
    }
    // Back rings disappear behind the opaque disc; front rings cross its face.
    // Stars are already in background and cannot shine through the solid sphere.
    let opacity = clamp(SUNSET_STRENGTH, 0.0, 1.0);
    var color = mix(background, ring_color, ring_alpha * opacity);
    color = mix(color, planet, sphere_mask * opacity);
    if ring_t < sphere_t && sphere_mask > 0.0 {
        color = mix(color, ring_color, ring_alpha * opacity);
    }
    return color;
}

// Called once from the sky fragment entry, before terrain/hand/UI render passes.
fn sunset_atmosphere(ray: vec3<f32>) -> vec3<f32> {
    // Derivatives must execute before any per-pixel early return.
    let footprint = max(length(dpdx(ray)), length(dpdy(ray)));
    let night = 1.0 - smoothstep(-0.10, 0.15, globals.sun.y);
    let visibility = mix(clamp(SUNSET_DAY_BLEND, 0.0, 1.0), 1.0, night);
    let horizon = smoothstep(-0.12, 0.16, ray.y);
    let blend = visibility * horizon;
    if blend <= 0.0 { return sky_along_ray(ray); }
    let time = globals.misc.y * clamp(SUNSET_SPEED, 0.0, 3.0);
    var color: vec3<f32>;
    if SUNSET_MODE == 1u {
        color = sunset_aurora(ray, time, footprint);
    } else {
        color = sunset_space(ray, time);
    }
    var stars = sunset_star_layer(ray, 113.0, 17, footprint, time);
    if SUNSET_QUALITY > 0u {
        stars += sunset_star_layer(ray, 237.0, 151, footprint, time) * 0.62;
    }
    if SUNSET_QUALITY > 1u {
        stars += sunset_star_layer(ray, 371.0, 401, footprint, time) * 0.28;
    }
    color += stars * clamp(SUNSET_STARS, 0.0, 2.0);
    if SUNSET_MODE == 2u {
        color = sunset_ringed_planet(ray, color, footprint, time);
    }
    // Compress bright cores without HDR alpha conventions or bloom dependencies.
    color = vec3(1.0) - exp(-max(color, vec3(0.0)) * clamp(SUNSET_BRIGHTNESS, 0.0, 2.0));
    // Meet the existing horizon/fog gently; do not recolor geometry or the lower void.
    // Native sky includes disc trigonometry and sunrise fan intersections. Avoid
    // evaluating it over the opaque custom sky, where its contribution is zero.
    if blend >= 1.0 { return color; }
    return mix(sky_along_ray(ray), color, blend);
}
"#;

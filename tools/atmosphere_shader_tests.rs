//! Standalone Cargo test target; dev dependency: naga 30.0.1 with wgsl-in,hlsl-out.
//! Set SUNSET_TERRAIN_WGSL to the pinned host's terrain.wgsl before `cargo test`.
#[path = "../mixins/sunset/atmosphere_shader.rs"]
mod shader;

fn section<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let offset = text.find(start).expect("pinned host section start");
    &text[offset..offset + text[offset..].find(end).expect("pinned host section end")]
}

#[test]
fn native_wgsl_and_all_specializations_validate_and_translate_to_hlsl() {
    let terrain = std::fs::read_to_string(
        std::env::var("SUNSET_TERRAIN_WGSL")
            .expect("set SUNSET_TERRAIN_WGSL to pinned terrain.wgsl"),
    )
    .unwrap();
    let source = [
        section(&terrain, "struct Globals {", "@group(0) @binding(1)"),
        section(&terrain, "fn linear_fog(", "fn apply_fog("),
        section(&terrain, "struct SkyOut {", "// rendertype_lines"),
        shader::ATMOSPHERE_WGSL,
        "@fragment fn fs_sunset_atmosphere(in: SkyOut) -> @location(0) vec4<f32> {\n\
             let ray = view_ray(in.ndc);\n\
             return vec4(sunset_atmosphere(ray, sky_along_ray(ray)), 1.0);\n}",
    ]
    .join("\n");
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    for mode in 0..=2 {
        for quality in 0..=2 {
            for boundary in 0..=1 {
                let mut values = naga::back::PipelineConstants::default();
                for (name, value) in [
                    ("SUNSET_MODE", f64::from(mode)),
                    ("SUNSET_QUALITY", f64::from(quality)),
                    ("SUNSET_BRIGHTNESS", if boundary == 0 { 0.2 } else { 2.0 }),
                    ("SUNSET_STRENGTH", f64::from(boundary) * 2.0),
                    ("SUNSET_STARS", f64::from(boundary) * 2.0),
                    ("SUNSET_SPEED", f64::from(boundary) * 2.0),
                    ("SUNSET_DAY_BLEND", f64::from(boundary)),
                ] {
                    values.insert(name.to_owned(), value);
                }
                let (specialized, info) = naga::back::pipeline_constants::process_overrides(
                    &module,
                    &info,
                    Some((naga::ShaderStage::Fragment, "fs_sunset_atmosphere")),
                    &values,
                )
                .unwrap();
                let options = naga::back::hlsl::Options::default();
                let pipeline_options = naga::back::hlsl::PipelineOptions::default();
                let mut hlsl = String::new();
                naga::back::hlsl::Writer::new(&mut hlsl, &options, &pipeline_options)
                    .write(&specialized, &info, None)
                    .unwrap();
                assert!(!hlsl.is_empty(), "mode={mode}, quality={quality}");
            }
        }
    }
}

fn vector_after(declaration: &str) -> [f64; 3] {
    shader::ATMOSPHERE_WGSL
        .split_once(declaration)
        .unwrap()
        .1
        .split_once(')')
        .unwrap()
        .0
        .split(',')
        .map(|value| value.trim().parse().unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

#[test]
fn precomputed_nebula_basis_preserves_angular_scale_and_handedness() {
    let n = vector_after("let normal = vec3(");
    let x = vector_after("let across = vec3(");
    let y = vector_after("let along = vec3(");
    for v in [n, x, y] {
        assert!((dot(v, v) - 1.0).abs() < 0.002);
    }
    for (a, b) in [(n, x), (n, y), (x, y)] {
        assert!(dot(a, b).abs() < 0.002);
    }
    let cross = [
        n[1] * x[2] - n[2] * x[1],
        n[2] * x[0] - n[0] * x[2],
        n[0] * x[1] - n[1] * x[0],
    ];
    assert!(dot(cross, y) > 0.999);
}

#[test]
fn aurora_azimuth_waves_have_no_longitude_branch_cut() {
    // atan2 changes by 2*pi on the same world direction. Integer angular
    // frequencies must agree there for every layer, animation phase and fold.
    for phase in [-1000.0_f64, -1.0, 0.0, 0.3, 91.0] {
        for layer in 0..3 {
            let wave = |a: f64| {
                let p = phase + f64::from(layer) * 2.1;
                let folds = (a * 9.0 + p + (a * 3.0 - p).sin() * 1.3).sin();
                ((a * 2.0 + p).sin(), (a * 113.0 + folds * 2.0 + p).sin())
            };
            let a = wave(-std::f64::consts::PI);
            let b = wave(std::f64::consts::PI);
            assert!((a.0 - b.0).abs() < 1e-10);
            assert!((a.1 - b.1).abs() < 1e-10);
        }
    }
}

#[test]
fn bounded_shader_work_and_no_new_resource_bindings() {
    // This catches accidental expansion into ray marching or extra frame data;
    // it complements native shader validation with the intended work budget.
    let text = shader::ATMOSPHERE_WGSL;
    assert!(text.contains("i < 6u"));
    assert!(text.contains("i < 3u"));
    assert_eq!(text.matches("for (").count(), 2);
    assert_eq!(text.matches("sunset_star_layer(ray,").count(), 3);
    assert!(!text.contains("@group"));
    assert!(!text.contains("textureSample"));
    assert!(!text.contains("while "));
}

fn solar_night_weight(solar_height: f64) -> f64 {
    let phase = ((solar_height + 0.10) / 0.25).clamp(0.0, 1.0);
    1.0 - phase * phase * (3.0 - 2.0 * phase)
}

#[test]
fn solar_dusk_blend_has_expected_endpoints_and_continuous_progression() {
    for (height, expected) in [
        (-1.0, 1.0),
        (-0.10, 1.0),
        (-0.05, 0.896),
        (0.0, 0.648),
        (0.025, 0.5),
        (0.05, 0.352),
        (0.15, 0.0),
        (1.0, 0.0),
    ] {
        assert!((solar_night_weight(height) - expected).abs() < 1e-12);
    }
    let mut previous = 1.0;
    for step in 0..=2000 {
        let height = f64::from(step) / 1000.0 - 1.0;
        let weight = solar_night_weight(height);
        assert!((0.0..=1.0).contains(&weight));
        assert!(weight <= previous);
        // Maximum slope is 6 / height-unit; at this sampling interval
        // no adjacent sample may jump by more than 0.006.
        assert!(previous - weight <= 0.0060001);
        previous = weight;
        for daytime in [0.0, 0.65, 1.0] {
            let visibility = daytime + (1.0 - daytime) * weight;
            assert!((daytime..=1.0).contains(&visibility));
        }
    }
    // The smoothstep joins both plateaus with zero first derivative.
    let epsilon = 1e-6;
    assert!((solar_night_weight(-0.10 + epsilon) - 1.0).abs() / epsilon < 0.001);
    assert!(solar_night_weight(0.15 - epsilon).abs() / epsilon < 0.001);
}

#[test]
fn shader_day_night_classifier_is_independent_of_weather_attenuated_stars() {
    // Native compilation alone accepts the old rain-sensitive expression.
    let classifier = shader::ATMOSPHERE_WGSL
        .lines()
        .find(|line| line.contains("let night ="))
        .expect("shader declares its day/night classifier");
    assert!(classifier.contains("globals.sun.y"), "{classifier}");
    assert!(!classifier.contains("globals.sun.w"), "{classifier}");
}

fn normalized(vector: [f64; 3]) -> [f64; 3] {
    let length = dot(vector, vector).sqrt();
    vector.map(|value| value / length)
}

fn sphere_distance(center: [f64; 3], ray: [f64; 3]) -> Option<f64> {
    let projection = dot(center, ray);
    let discriminant = projection * projection - (dot(center, center) - 1.0);
    (projection > 0.0 && discriminant >= 0.0).then(|| projection - discriminant.sqrt())
}

fn ring_intersection(center: [f64; 3], axis: [f64; 3], ray: [f64; 3]) -> Option<(f64, f64)> {
    let denominator = dot(ray, axis);
    if denominator.abs() <= 0.0001 {
        return None;
    }
    let distance = dot(center, axis) / denominator;
    if distance <= 0.0 || distance >= 12.0 {
        return None;
    }
    let relative = std::array::from_fn(|i| ray[i] * distance - center[i]);
    Some((distance, dot(relative, relative).sqrt()))
}

#[test]
fn planet_has_ten_degree_silhouette_and_occludes_background_stars() {
    let center = vector_after("let planet_center = vec3(");
    let direction = normalized(center);
    assert!((sphere_distance(center, direction).unwrap() - 5.0).abs() < 1e-6);
    assert!(sphere_distance(center, direction.map(|value| -value)).is_none());
    let side = normalized([direction[2], 0.0, -direction[0]]);
    for degrees in [0.0_f64, 3.0, 9.0, 9.5, 9.7, 10.0, 12.0] {
        let angle = degrees.to_radians();
        let ray = std::array::from_fn(|i| direction[i] * angle.cos() + side[i] * angle.sin());
        assert_eq!(sphere_distance(center, ray).is_some(), degrees <= 9.5);
    }
    // The star field must be evaluated before the opaque planet replacement.
    // Otherwise the geometry can be correct while stars shine through its face.
    let text = shader::ATMOSPHERE_WGSL;
    let entry = text.split_once("fn sunset_atmosphere(").unwrap().1;
    assert!(
        entry.find("color += stars").unwrap() < entry.find("color = sunset_ringed_planet").unwrap()
    );
    assert!(text.contains("color = mix(color, planet, sphere_mask * opacity)"));
}

#[test]
fn tilted_ring_has_both_front_and_back_sphere_intersections() {
    let center = vector_after("let planet_center = vec3(");
    let axis = vector_after("let axis = vec3(");
    assert!((dot(axis, axis) - 1.0).abs() < 1e-6);
    let direction = normalized(center);
    let alignment = dot(direction, axis);
    let toward_camera = normalized(std::array::from_fn(|i| -direction[i] + alignment * axis[i]));
    for (sign, is_front) in [(1.0, true), (-1.0, false)] {
        let ring_point = std::array::from_fn(|i| center[i] + toward_camera[i] * 1.5 * sign);
        let ray = normalized(ring_point);
        let (distance, radius) = ring_intersection(center, axis, ray).unwrap();
        assert!((radius - 1.5).abs() < 1e-6);
        let sphere = sphere_distance(center, ray).expect("ring crosses projected opaque disc");
        assert_eq!(distance < sphere, is_front);
    }
}

#[test]
fn ring_ray_tests_reject_parallel_backward_and_unbounded_hits() {
    let center = vector_after("let planet_center = vec3(");
    let axis = vector_after("let axis = vec3(");
    let parallel = normalized([axis[2], 0.0, -axis[0]]);
    assert!(ring_intersection(center, axis, parallel).is_none());
    assert!(ring_intersection(center, axis, axis.map(|value| -value)).is_none());
    let almost_parallel = normalized(std::array::from_fn(|i| parallel[i] + axis[i] * 0.0002));
    assert!(ring_intersection(center, axis, almost_parallel).is_none());
    for x in -10..=10 {
        for y in -10..=10 {
            for z in -10..=10 {
                if x == 0 && y == 0 && z == 0 {
                    continue;
                }
                let ray = normalized([f64::from(x), f64::from(y), f64::from(z)]);
                if let Some((distance, radius)) = ring_intersection(center, axis, ray) {
                    assert!(distance.is_finite() && distance > 0.0 && distance < 12.0);
                    assert!(radius.is_finite() && radius >= 0.0);
                    let point = ray.map(|value| value * distance);
                    assert!((dot(point, axis) - dot(center, axis)).abs() < 1e-10);
                }
            }
        }
    }
    let planet = section(
        shader::ATMOSPHERE_WGSL,
        "fn sunset_ringed_planet(",
        "// Called once",
    );
    assert!(!planet.contains("dpdx") && !planet.contains("dpdy"));
    assert!(!planet.contains("for (") && !planet.contains("while "));
}

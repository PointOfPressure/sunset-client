"""Prepare identical shader inputs and a local Cargo patch config; no GPU access."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct

HERE = Path(__file__).resolve().parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def section(text, start, end):
    return start + text.split(start, 1)[1].split(end, 1)[0]


def shader_directory(path):
    """Accept either a Sunset checkout/bundle or its mixins/sunset directory."""
    path = path.resolve()
    if (path / "atmosphere_shader.rs").is_file():
        return path
    return path / "mixins" / "sunset"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--horizon", required=True, type=Path, help="Pinned Horizon checkout root")
    parser.add_argument("--original", required=True, type=Path, help="Original Sunset root or mixins/sunset")
    parser.add_argument("--optimized", required=True, type=Path, help="Optimized Sunset root or mixins/sunset")
    parser.add_argument("--output", type=Path, default=HERE / "fixture", help="Generated fixture directory")
    args = parser.parse_args()
    horizon = args.horizon.resolve()
    terrain_path = horizon / "crates/horizon-client-render/src/terrain.wgsl"
    hal = horizon / "third_party/wgpu-hal"
    original = shader_directory(args.original)
    optimized = shader_directory(args.optimized)
    output = args.output.resolve()
    required = [terrain_path, hal / "Cargo.toml"]
    required += [directory / name for directory in [original, optimized]
                 for name in ["atmosphere_shader.rs", "atmosphere_renderer.rs"]]
    for path in required:
        if not path.is_file():
            parser.error(f"Required pinned input missing: {path}")
    if (output / "gpu-samples.csv").exists():
        parser.error("Output already contains measured samples; choose a fresh --output directory")
    output.mkdir(parents=True, exist_ok=True)

    # Local paths live in generated config, so Cargo.toml/lock remain portable.
    config_path = output / "cargo-config.toml"
    config_path.write_text('[patch.crates-io]\nwgpu-hal = { path = '
                           + json.dumps(hal.as_posix()) + ' }\n', encoding="utf-8")
    terrain = terrain_path.read_text(encoding="utf-8")
    prefix = "\n".join(section(terrain, start, end) for start, end in [
        ("struct Globals {", "@group(0) @binding(1)"),
        ("fn linear_fog(", "fn apply_fog("),
        ("struct SkyOut {", "// rendertype_lines"),
    ])
    sources = [terrain_path]
    for label, directory in [("original", original), ("optimized", optimized)]:
        shader_path = directory / "atmosphere_shader.rs"
        entry_path = directory / "atmosphere_renderer.rs"
        shader = shader_path.read_text(encoding="utf-8").split('r#"', 1)[1].split('"#;', 1)[0]
        entry = entry_path.read_text(encoding="utf-8").split('const ENTRY: &str = "', 1)[1].split('";', 1)[0]
        (output / (label + ".wgsl")).write_text(prefix + "\n" + shader + "\n" + entry, encoding="utf-8")
        sources.extend([shader_path, entry_path])
    (output / "native.wgsl").write_text(prefix, encoding="utf-8")

    # Same explicit synthetic night fixture as the measured 2026-10-31 run.
    # This is not a byte-for-byte capture of any game's live Globals buffer.
    yaw, pitch = map(math.radians, [-35.264, -30])
    sy, cy, sp, cp = math.sin(yaw), math.cos(yaw), math.sin(pitch), math.cos(pitch)
    right = [-cy, 0, -sy, 0]
    up = [-sy * sp, cp, cy * sp, 0]
    minus_forward = [sy * cp, sp, -cy * cp, 0]
    inverse = right + up + minus_forward + [0, 0, 0, 1]
    identity = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
    values = identity + [.015, .018, .03, 1] + [0, 256, 192, 256] + [.005, .008, .02, 1]
    values += [1, 18000, 2000 / 1125, math.tan(math.radians(35))] + inverse
    values += [0, 0, 0, 0] + [0, -1, 0, .5] + [256, 2048, 0, 0]
    values += [0] * 8 + [.75, 0, 0, 0] + [0] * 4 + [0] * 4 + [0] * 64 + [0] * 128
    assert len(values) == 272
    (output / "globals.bin").write_bytes(struct.pack("<272f", *values))
    generated = [output / name for name in ["original.wgsl", "optimized.wgsl", "native.wgsl", "globals.bin", "cargo-config.toml"]]
    harness = [HERE / name for name in ["src/main.rs", "Cargo.toml", "Cargo.lock", "prepare.py", "run.py", "summarize.py"]]
    provenance = {
        "kind": "surface-free-shader-only-gpu-microbenchmark",
        "source_files": {str(path): sha(path) for path in sources},
        "fixture_files": {str(path): sha(path) for path in generated},
        "harness_files": {str(path): sha(path) for path in harness},
        "globals_provenance": "Synthetic explicit reconstruction matching documented native camera/FOV/resolution/night setup; NOT a captured game Globals buffer. Sky/fog colors are fixed plausible night inputs, identical for both shaders.",
        "globals_f32": values,
        "scene": {"framebuffer": [2000, 1125], "yaw_pitch_degrees": [-35.264, -30], "vertical_fov_degrees": 70, "sun": [0, -1, 0, .5], "sunrise_alpha": 0, "speed": 0, "brightness": 1, "effect_strength": 1, "stars": 1, "day_blend": .65},
        "raster": {"vertex": "pinned native vs_sky; draw(0..3,0..1)", "target": "Bgra8Unorm", "depth": "Depth32Float; Always; no writes", "samples": 1, "surface": None, "downsampling": False},
        "timing_scope": "Timestamp begin/end each complete sky-only render pass, including identical attachment clear/store; excludes host/game/terrain/HUD/CPU/submission/readback time. Native sky baseline is also shader-only.",
        "order": "Default eight paired rounds rotate case order and alternate original/optimized first. At least 600 ms sustained alternating original/optimized GPU warmup immediately before EVERY 64-sample block; wall duration recorded. All pipelines also prewarmed initially. Warmup/copies/readback excluded from GPU intervals.",
        "limitations": ["No whole-game FPS or total frame GPU performance claim.", "Synthetic uniform fixture, not a captured game frame.", "All sky pixels render without terrain, as the native fullscreen sky triangle does.", "No enforced GPU clock lock or exclusive-device scheduling; nvidia-smi metadata records observed state."],
    }
    (output / "provenance.json").write_text(json.dumps(provenance, indent=2), encoding="utf-8")
    print("Prepared CPU-only fixture:", output)
    print("Cargo patch config:", config_path)
    print("No GPU device created; GO_GPU_RENDER must be created explicitly before running.")


if __name__ == "__main__":
    main()

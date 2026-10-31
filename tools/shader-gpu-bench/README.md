# Surface-free sky shader GPU benchmark

This standalone tool compares original and optimized Atmosphere shaders on the
same GPU without starting Horizon, Winit, a window, or a presentation surface.
It measures **sky-only render-pass GPU time**, not whole-game FPS, CPU time, or
total frame GPU time. Run it separately from games and other GPU benchmarks.

The measured Rust source is preserved byte for byte from the 2026-10-31 test:
`src/main.rs` SHA-256
`56eab46ceeb3261c8174a475b1654d6e939d3fbe7a4029bb0a5a0f209af6e87e`.
Dependency versions/features and `Cargo.lock` are retained. The original local
absolute `wgpu-hal` patch path is supplied through generated Cargo configuration
instead, so another Horizon checkout can be used without editing the manifest.

## Inputs and scope

- Pinned Horizon checkout `ba6344a`, including `third_party/wgpu-hal`; Rust
  toolchain compatible with the pinned host (the recorded run used 1.99.1).
- Two Sunset checkouts/bundles containing `mixins/sunset/atmosphere_shader.rs`
  and `atmosphere_renderer.rs`. Preparation also accepts those `mixins/sunset`
  directories directly.
- Vulkan hardware adapter with timestamp queries; no software fallback.
- Python 3. Pillow is needed only to encode PNGs; `--skip-png` keeps raw BGRA
  captures and timing summaries without it. NVIDIA's `nvidia-smi`, when present,
  supplies clock, temperature, power, P-state and utilization metadata.

Preparation extracts the exact native `Globals`, sky functions, `vs_sky`, each
shader and its fragment entry from the passed sources. Both variants receive
the same explicit synthetic 1,088-byte Globals fixture: 2000×1125 pixels,
yaw −35.264°, pitch −30°, vertical FOV 70°, night sun direction, speed zero,
brightness/effect/stars one. **This is not a captured game Globals buffer.**
The sky/fog colors are fixed plausible night values; every uniform is recorded
in `provenance.json` along with input and harness hashes.

The native full-screen triangle renders at full resolution into `Bgra8Unorm`
with `Depth32Float`, one sample, depth comparison Always and no depth writes.
No resolution reduction, terrain, HUD, presentation, or additional game pass is
involved. Timestamp queries bracket each sky render pass, including identical
attachment clear/store work. Warmup, command submission, timestamp resolution,
CPU waits and image readback are outside those GPU intervals. A separate native
sky shader baseline uses the same fixture; it is not a full-game baseline.

## Prepare and compile (CPU only)

From the Sunset repository root, for example in PowerShell:

```powershell
python tools/shader-gpu-bench/prepare.py --horizon C:/src/horizon --original C:/src/sunset-original --optimized . --output D:/shader-bench-run
cargo --config D:/shader-bench-run/cargo-config.toml build --locked --manifest-path tools/shader-gpu-bench/Cargo.toml --target-dir D:/shader-bench-target
```

Use the pinned compiler/cargo environment for the host. `--offline` can be added
when dependencies are already cached. Output defaults to this tool's ignored
`fixture/` directory if `--output` is omitted. An existing directory containing
measured samples is rejected to preserve its evidence. Preparation and
compilation do not create a GPU device or run any draws.

## Explicitly enable and run

The Rust binary checks for `GO_GPU_RENDER` **before creating an instance or
device**. Once ready to run the GPU workload, create that file manually:

```powershell
Set-Content -LiteralPath D:/shader-bench-run/GO_GPU_RENDER -Value 'Run this GPU benchmark now'
python tools/shader-gpu-bench/run.py --fixture D:/shader-bench-run --binary D:/shader-bench-target/debug/sunset-shader-gpu-bench.exe
```

On another platform, create the same file and pass the platform's executable
path (without `.exe`). The wrapper records the binary hash, verifies frozen
inputs, streams the log, samples `nvidia-smi` every 200 ms, stops its own telemetry
process and consumes the gate when finished. It does not change GPU clock or
power settings. Use `--skip-png` when Pillow is unavailable.

Defaults are eight paired rounds for Space, Aurora and Ringed Planet at both
Balanced and Performance. Case order rotates and original/optimized ordering
alternates. Immediately before **each** 64-sample block, original and optimized
pipelines alternate for at least **600 ms of sustained GPU warmup**; actual wall
duration is logged. This gives **512 timed samples per variant per case**, plus
128 native shader baseline samples. `BENCH_ROUNDS` and `BENCH_SAMPLES` can
override those counts, which the adapter/execution records retain.

## Evidence and interpretation

The output directory contains raw per-draw `gpu-samples.csv`, paired-round
distributions in `summary.json`, input/harness fingerprints in `provenance.json`,
the executed binary hash and exit status in `execution.json`, `adapter.txt`,
`warm-blocks.csv`, round telemetry snapshots, continuous `nvidia-timeline.csv`
and the console log. Twelve `.bgra` files are exact GPU readbacks; optional PNGs
encode those bytes without crop, scaling, compositing or color adjustment.
Generated fixtures, samples and captures are not committed to the repository.

The summarizer verifies source hashes and complete sample/round counts before
reporting medians, means, p95/p99 and per-round changes. It can be rerun with:

```powershell
python tools/shader-gpu-bench/summarize.py --fixture D:/shader-bench-run
```

Mobile DVFS can still vary graphics clocks materially despite warmup. The
recorded GTX 1650 Ti run observed 660–1905 MHz during the measurement window.
Alternating paired rounds reduce order and thermal bias; they do not lock or
normalize clocks, guarantee exclusive GPU scheduling, or establish portable
FPS gains. Report clock variance and the range of paired-round reductions
alongside shader times. Compare screenshots too: lower quality deliberately
changes nebula detail and Aurora curtain count.

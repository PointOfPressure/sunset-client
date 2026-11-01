// Specializes only Horizon's resident vanilla sky pipeline. The existing world pass,
// group 0 Globals buffer, camera, celestial bodies, depth and target remain Horizon's.
// Unchanged frames add no allocation, buffer upload, draw call or render pass.
use std::cell::RefCell;
use std::time::{Duration, Instant};

use super::sunset_atmosphere_flags::{Config, snapshot};
use super::wgsl_prefix::{TERRAIN, section};
use super::{Frame, Renderer};

const DEBOUNCE: Duration = Duration::from_millis(250);
const ENTRY: &str = "
@fragment fn fs_sunset_atmosphere(in: SkyOut) -> @location(0) vec4<f32> {
    let ray = view_ray(in.ndc);
    return vec4(sunset_atmosphere(ray), 1.0);
}
";

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key([u32; 7]);

impl From<Config> for Key {
    fn from(c: Config) -> Self {
        Self([
            c.mode,
            c.brightness.to_bits(),
            c.strength.to_bits(),
            c.stars.to_bits(),
            c.speed.to_bits(),
            c.day_blend.to_bits(),
            c.quality,
        ])
    }
}

struct State {
    device: wgpu::Device,
    globals: wgpu::Buffer,
    format: wgpu::TextureFormat,
    original: wgpu::RenderPipeline,
    shader: Option<wgpu::ShaderModule>,
    effect: Option<(Key, wgpu::RenderPipeline)>,
    requested: Key,
    changed_at: Instant,
    failed: Option<Key>,
    active: bool,
}

// Horizon has one active renderer on the window thread. The release_surface and
// Game::finish_perf HEAD hooks drop this cache while Renderer/Instance are still alive.
// GPU resources must never survive until the window thread's TLS destructors.
thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn shader(device: &wgpu::Device) -> Option<wgpu::ShaderModule> {
    let text = [
        section(TERRAIN, "struct Globals {", "@group(0) @binding(1)"),
        section(TERRAIN, "fn linear_fog(", "fn apply_fog("),
        section(TERRAIN, "struct SkyOut {", "// rendertype_lines"),
        super::sunset_atmosphere_shader::ATMOSPHERE_WGSL,
        ENTRY,
    ]
    .concat();
    let module = match naga::front::wgsl::parse_str(&text) {
        Ok(module) => module,
        Err(error) => {
            log::warn!("Sunset sky disabled: {}", error.emit_to_string(&text));
            return None;
        }
    };
    if let Err(error) = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    {
        log::warn!("Sunset sky disabled: {error}");
        return None;
    }
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Sunset original procedural sky"),
        source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
    });
    if let Some(error) = pollster::block_on(scope.pop()) {
        log::warn!("Sunset sky disabled: {error}");
        return None;
    }
    Some(module)
}

fn pipeline(
    renderer: &Renderer,
    shader: &wgpu::ShaderModule,
    config: Config,
) -> Option<wgpu::RenderPipeline> {
    let constants = [
        ("SUNSET_MODE", f64::from(config.mode)),
        ("SUNSET_BRIGHTNESS", f64::from(config.brightness)),
        ("SUNSET_STRENGTH", f64::from(config.strength)),
        ("SUNSET_STARS", f64::from(config.stars)),
        ("SUNSET_SPEED", f64::from(config.speed)),
        ("SUNSET_DAY_BLEND", f64::from(config.day_blend)),
        ("SUNSET_QUALITY", f64::from(config.quality)),
    ];
    let device = &renderer.device;
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Sunset sky existing terrain globals"),
        bind_group_layouts: &[Some(&renderer.terrain_layout)],
        ..Default::default()
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Sunset Space / Aurora sky"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_sky"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_sunset_atmosphere"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                ..Default::default()
            },
            targets: &[Some(wgpu::ColorTargetState {
                format: renderer.config.format.remove_srgb_suffix(),
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    if let Some(error) = pollster::block_on(scope.pop()) {
        log::warn!("Sunset sky settings unavailable; using vanilla sky: {error}");
        return None;
    }
    Some(pipeline)
}

fn supported(renderer: &Renderer, frame: &Frame<'_>, config: Config) -> bool {
    config.enabled
        && config.world_supported
        && !renderer.suspended
        && !frame.title
        && frame.skybox
        && !frame.end_sky
        && !frame.nether_lighting
        && !frame.sky_blocked
        && !frame.underwater
        && frame.pack.eye_in == 0
        && frame.environment_fog.is_none()
        && renderer.native.is_none()
        && renderer.pack_post.is_none()
}

pub(super) fn prepare(renderer: &mut Renderer, frame: &Frame<'_>) {
    let config = snapshot();
    let enabled = supported(renderer, frame, config);
    STATE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if let Some(state) = slot.as_mut() {
            state.active = false;
        }
        if let Some(state) = slot.as_ref()
            && (state.device != renderer.device
                || state.globals != renderer.globals
                || state.format != renderer.config.format.remove_srgb_suffix())
        {
            // A renderer replacing its device already has its own vanilla pipeline.
            *slot = None;
        }
        if !enabled {
            if let Some(state) = slot.as_ref() {
                renderer.sky = state.original.clone();
            }
            return;
        }
        let key = Key::from(config);
        let state = slot.get_or_insert_with(|| State {
            device: renderer.device.clone(),
            globals: renderer.globals.clone(),
            format: renderer.config.format.remove_srgb_suffix(),
            original: renderer.sky.clone(),
            shader: shader(&renderer.device),
            effect: None,
            requested: key,
            changed_at: Instant::now(),
            failed: None,
            active: false,
        });
        if state.requested != key {
            state.requested = key;
            state.changed_at = Instant::now();
        }
        let current = state.effect.as_ref().map(|(key, _)| *key);
        // Keep the prior effect during a slider drag; compile only the settled value.
        let settled = current.is_none() || state.changed_at.elapsed() >= DEBOUNCE;
        if current != Some(key) && state.failed != Some(key) && settled {
            if let Some(shader) = state.shader.as_ref() {
                match pipeline(renderer, shader, config) {
                    Some(pipeline) => {
                        state.effect = Some((key, pipeline));
                        state.failed = None;
                    }
                    None => {
                        state.failed = Some(key);
                        state.effect = None;
                        // A validated, bounded specialization failing on this device is
                        // terminal until renderer replacement; slider edits cannot retry it.
                        state.shader = None;
                    }
                }
            }
        }
        renderer.sky = state
            .effect
            .as_ref()
            .map_or_else(|| state.original.clone(), |(_, pipeline)| pipeline.clone());
        state.active = state.effect.is_some();
    });
}

pub(super) fn owns_sky() -> bool {
    STATE.with(|cell| cell.borrow().as_ref().is_some_and(|state| state.active))
}

pub fn release(renderer: &mut Renderer) {
    // Shutdown may be exceptional; never attempt to re-enter an already-destroyed TLS.
    let _ = STATE.try_with(|cell| {
        let mut slot = cell.borrow_mut();
        if let Some(state) = slot.as_ref()
            && state.device == renderer.device
            && state.globals == renderer.globals
        {
            renderer.sky = state.original.clone();
            *slot = None;
        }
    });
}

// Sunset's own UI pipeline inside Horizon's frame: the shapes of ::sunset_core::ui (rounded rectangles by signed
// distance: anti-aliased edges, gradients, borders, shadows, a blur of the frame behind) drawn in passes of their own
// between parts of the HUD (the fork's gfx::hud_cut), so the HUD's text drawn after a layer still goes over it. It
// stands beside the HudBatch path (runtime_draw.rs paint), which draws the same shapes as triangles wherever this
// can't: until the first frame says it can, on a layered (HDR) frame, or if the device refuses the shader.
use std::cell::{Cell, RefCell};
use std::sync::atomic::Ordering;

use ::sunset_core::ui::{self, Layers, PRIM_FLOATS};
use horizon_client::gfx::HudCut;

const SHADER: &str = "
struct Globals {
    // The target's width and height in pixels, pixels per unit, and 1 where the blurred frame is bound.
    size: vec4<f32>,
    // A blur pass's step between taps, in texture coordinates.
    dir: vec4<f32>,
}
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var behind: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;

struct Shape {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) half: vec2<f32>,
    @location(2) @interpolate(flat) radii: vec4<f32>,
    @location(3) @interpolate(flat) fill: vec4<f32>,
    @location(4) @interpolate(flat) fill2: vec4<f32>,
    @location(5) @interpolate(flat) border: vec4<f32>,
    @location(6) @interpolate(flat) clip: vec4<f32>,
    @location(7) @interpolate(flat) params: vec4<f32>,
    @location(8) @interpolate(flat) extra: vec4<f32>,
}

@vertex fn vs_shape(
    @builtin(vertex_index) index: u32,
    @location(0) rect_in: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) fill2: vec4<f32>,
    @location(4) border: vec4<f32>,
    @location(5) clip: vec4<f32>,
    @location(6) params: vec4<f32>,
    @location(7) extra: vec4<f32>,
) -> Shape {
    let scale = g.size.z;
    var lo = rect_in.xy;
    var hi = rect_in.xy + rect_in.zw;
    // A plain rectangle keeps hard edges on whole pixels, as the HUD's own fills have.
    if radii.x + radii.y + radii.z + radii.w == 0.0 && params.y == 0.0 {
        lo = round(lo * scale) / scale;
        hi = round(hi * scale) / scale;
    }
    let spread = params.y * 2.0 + 1.0 / scale;
    let corner = vec2(f32(index & 1u), f32(index >> 1u));
    let at = mix(lo - vec2(spread), hi + vec2(spread), corner);
    let px = at * scale;
    var out: Shape;
    out.pos = vec4(px.x / g.size.x * 2.0 - 1.0, 1.0 - px.y / g.size.y * 2.0, 0.0, 1.0);
    out.local = at - (lo + hi) * 0.5;
    out.half = (hi - lo) * 0.5;
    out.radii = radii;
    out.fill = fill;
    out.fill2 = fill2;
    out.border = border;
    out.clip = clip * scale;
    out.params = params;
    out.extra = extra;
    return out;
}

// The distance to a box whose corners are rounded with power n (2: circles, 4: a squircle's).
fn distance_to(p: vec2<f32>, half: vec2<f32>, radii: vec4<f32>, n: f32) -> f32 {
    var r = radii.x;
    if p.x >= 0.0 && p.y < 0.0 {
        r = radii.y;
    } else if p.x >= 0.0 && p.y >= 0.0 {
        r = radii.z;
    } else if p.y >= 0.0 {
        r = radii.w;
    }
    r = min(r, min(half.x, half.y));
    let q = abs(p) - half + vec2(r);
    let m = max(q, vec2(0.0));
    var len = length(m);
    if n != 2.0 {
        len = pow(pow(m.x, n) + pow(m.y, n), 1.0 / n);
    }
    return min(max(q.x, q.y), 0.0) + len - r;
}

@fragment fn fs_shape(in: Shape) -> @location(0) vec4<f32> {
    let px = in.pos.xy;
    if px.x < in.clip.x || px.y < in.clip.y || px.x >= in.clip.z || px.y >= in.clip.w {
        discard;
    }
    let scale = g.size.z;
    let d = distance_to(in.local, in.half, in.radii, in.params.w) * scale;
    let opacity = in.extra.z;
    if in.params.y > 0.0 {
        // A shadow: the shape's edge spread over the blur's width.
        let width = in.params.y * scale;
        let a = in.fill.a * (1.0 - smoothstep(-width, width, d)) * opacity;
        return vec4(in.fill.rgb * a, a);
    }
    let cover = clamp(0.5 - d, 0.0, 1.0);
    if cover <= 0.0 {
        discard;
    }
    var fill = in.fill;
    let dir = in.extra.xy;
    if dir.x != 0.0 || dir.y != 0.0 {
        let span = abs(dir.x) * in.half.x + abs(dir.y) * in.half.y;
        fill = mix(in.fill, in.fill2, clamp(dot(in.local, dir) / max(2.0 * span, 0.0001) + 0.5, 0.0, 1.0));
    }
    var rgb = fill.rgb;
    var a = fill.a;
    if in.params.z > 0.0 && g.size.w > 0.5 {
        // The blurred frame shows through what the fill leaves see-through.
        let back = textureSampleLevel(behind, smp, px / g.size.xy, 0.0).rgb;
        let show = (1.0 - a) * in.params.z;
        rgb = (rgb * a + back * show) / max(a + show, 0.0001);
        a = a + show;
    }
    let width = in.params.x * scale;
    if width > 0.0 {
        // The border, over the fill, from the edge inwards.
        let k = (1.0 - clamp(0.5 - (d + width), 0.0, 1.0)) * in.border.a;
        let both = k + a * (1.0 - k);
        rgb = (in.border.rgb * k + rgb * a * (1.0 - k)) / max(both, 0.0001);
        a = both;
    }
    a = a * cover * opacity;
    return vec4(rgb * a, a);
}

struct Screen {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@vertex fn vs_blur(@builtin(vertex_index) index: u32) -> Screen {
    let corner = vec2(f32((index << 1u) & 2u), f32(index & 2u));
    var out: Screen;
    out.pos = vec4(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2(corner.x, 1.0 - corner.y);
    return out;
}
const WEIGHTS = array<f32, 4>(0.1802, 0.1238, 0.0663, 0.0276);
// Nine taps of a bell curve along dir.
@fragment fn fs_blur(in: Screen) -> @location(0) vec4<f32> {
    var sum = textureSampleLevel(behind, smp, in.uv, 0.0).rgb * 0.2042;
    for (var i = 0; i < 4; i++) {
        let off = g.dir.xy * f32(i + 1);
        sum += (textureSampleLevel(behind, smp, in.uv + off, 0.0).rgb + textureSampleLevel(behind, smp, in.uv - off, 0.0).rgb) * WEIGHTS[i];
    }
    return vec4(sum, 1.0);
}
";

const STRIDE: u64 = (PRIM_FLOATS * 4) as u64;
// The blur works at a quarter of the frame's size: four passes, the last two with wider steps.
const SHRINK: u32 = 4;
const PASSES: [[f32; 2]; 4] = [[1.0, 0.0], [0.0, 1.0], [2.0, 0.0], [0.0, 2.0]];

struct Blur {
    size: [u32; 2],
    source: wgpu::TextureFormat,
    copy: wgpu::Texture,
    // Each pass's bindings and the view it draws into; the last view holds the blurred frame.
    passes: Vec<(wgpu::Buffer, wgpu::BindGroup, wgpu::TextureView)>,
    // The shapes' bindings with the blurred frame.
    globals: wgpu::Buffer,
    group: wgpu::BindGroup,
}

struct State {
    device: wgpu::Device,
    format: wgpu::TextureFormat,
    shapes: wgpu::RenderPipeline,
    blur_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    // The shapes' bindings without a blurred frame.
    globals: wgpu::Buffer,
    group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: u64,
    uploaded: Option<u64>,
    blur: Option<Blur>,
    // A frame size and format the blur's textures could not be made for: not tried again.
    no_blur: Option<([u32; 2], wgpu::TextureFormat)>,
}

// The window thread's, like the sky's cache: dropped by release() while the renderer still lives.
thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static FAILED: Cell<bool> = const { Cell::new(false) };
}

fn checked<T>(device: &wgpu::Device, filter: wgpu::ErrorFilter, what: &str, make: impl FnOnce() -> T) -> Option<T> {
    let scope = device.push_error_scope(filter);
    let made = make();
    match pollster::block_on(scope.pop()) {
        Some(error) => {
            log::warn!("Sunset UI pipeline: {what} unavailable: {error}");
            None
        }
        None => Some(made),
    }
}

fn uniform(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Sunset UI globals"),
        size: 32,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn bytes(floats: &[f32]) -> Vec<u8> {
    floats.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

fn texture(device: &wgpu::Device, size: [u32; 2], format: wgpu::TextureFormat, usage: wgpu::TextureUsages, view_formats: &[wgpu::TextureFormat]) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Sunset UI"),
        size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats,
    })
}

impl State {
    fn group(&self, globals: &wgpu::Buffer, view: &wgpu::TextureView) -> wgpu::BindGroup {
        group(&self.device, &self.layout, &self.sampler, globals, view)
    }

    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Option<Self> {
        checked(device, wgpu::ErrorFilter::Validation, "the shader", || {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Sunset UI"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Sunset UI bindings"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Sunset UI"),
                bind_group_layouts: &[Some(&layout)],
                ..Default::default()
            });
            let attributes = wgpu::vertex_attr_array![
                0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4,
                4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4
            ];
            let pipeline = |label, vertex, fragment, buffers: &[Option<wgpu::VertexBufferLayout<'_>>], blend, topology| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState { module: &shader, entry_point: Some(vertex), buffers, compilation_options: Default::default() },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(fragment),
                        targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState { topology, ..Default::default() },
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
            };
            let shapes = pipeline(
                "Sunset UI shapes",
                "vs_shape",
                "fs_shape",
                &[Some(wgpu::VertexBufferLayout { array_stride: STRIDE, step_mode: wgpu::VertexStepMode::Instance, attributes: &attributes })],
                Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                wgpu::PrimitiveTopology::TriangleStrip,
            );
            let blur_pipeline = pipeline("Sunset UI blur", "vs_blur", "fs_blur", &[], None, wgpu::PrimitiveTopology::TriangleList);
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Sunset UI"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                ..Default::default()
            });
            let globals = uniform(device);
            // Bound where there is no blurred frame; the shader does not read it then.
            let blank = texture(device, [1, 1], format, wgpu::TextureUsages::TEXTURE_BINDING, &[]).create_view(&Default::default());
            let capacity = 256;
            let instances = instance_buffer(device, capacity);
            let group = group(device, &layout, &sampler, &globals, &blank);
            Self {
                device: device.clone(),
                format,
                shapes,
                blur_pipeline,
                layout,
                sampler,
                globals,
                group,
                instances,
                capacity,
                uploaded: None,
                blur: None,
                no_blur: None,
            }
        })
    }

    fn make_blur(&self, size: [u32; 2], source: wgpu::TextureFormat) -> Option<Blur> {
        let device = &self.device;
        checked(device, wgpu::ErrorFilter::OutOfMemory, "the blur's room", || {
            checked(device, wgpu::ErrorFilter::Validation, "the blur", || {
                // Read as the HUD wrote it: without the frame format's own sRGB decoding.
                let plain = source.remove_srgb_suffix();
                let view_formats: &[wgpu::TextureFormat] = if plain == source { &[] } else { std::slice::from_ref(&plain) };
                let copy = texture(device, size, source, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats);
                let copy_view = copy.create_view(&wgpu::TextureViewDescriptor { format: Some(plain), ..Default::default() });
                let small = size.map(|s| s.div_ceil(SHRINK).max(1));
                let usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT;
                let targets: [wgpu::TextureView; 2] = std::array::from_fn(|_| texture(device, small, self.format, usage, &[]).create_view(&Default::default()));
                // The frame into the first, then to and fro; the fourth pass ends in the second.
                let mut source_view = &copy_view;
                let mut passes = Vec::new();
                for (i, _) in PASSES.iter().enumerate() {
                    let buffer = uniform(device);
                    let group = self.group(&buffer, source_view);
                    let target = &targets[i % 2];
                    passes.push((buffer, group, target.clone()));
                    source_view = target;
                }
                let globals = uniform(device);
                let group = self.group(&globals, source_view);
                Blur { size, source, copy, passes, globals, group }
            })
        })
        .flatten()
    }

    // The frame so far, blurred, for the shapes that show what is behind them. False where it can't be had.
    fn blur_behind(&mut self, ctx: &mut HudCut<'_>) -> bool {
        let Some(frame) = ctx.frame else {
            return false;
        };
        let size = [frame.width(), frame.height()];
        if frame.sample_count() != 1 || !frame.usage().contains(wgpu::TextureUsages::COPY_SRC) || size != ctx.size || size.contains(&0) {
            return false;
        }
        let key = (size, frame.format());
        if self.no_blur == Some(key) {
            return false;
        }
        if self.blur.as_ref().is_none_or(|b| (b.size, b.source) != key) {
            self.blur = self.make_blur(size, frame.format());
            if self.blur.is_none() {
                self.no_blur = Some(key);
                return false;
            }
        }
        let Some(blur) = &self.blur else {
            return false;
        };
        ctx.encoder.copy_texture_to_texture(
            frame.as_image_copy(),
            blur.copy.as_image_copy(),
            wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        );
        let small = size.map(|s| s.div_ceil(SHRINK).max(1));
        for ((buffer, group, target), step) in blur.passes.iter().zip(PASSES) {
            let dir = [step[0] / small[0] as f32, step[1] / small[1] as f32];
            ctx.queue.write_buffer(buffer, 0, &bytes(&[0.0, 0.0, 0.0, 0.0, dir[0], dir[1], 0.0, 0.0]));
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })];
            let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Sunset UI blur"),
                color_attachments: &attachments,
                ..Default::default()
            });
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        }
        true
    }

    fn draw(&mut self, ctx: &mut HudCut<'_>, layers: &Layers) {
        let Some(cut) = layers.cuts.iter().find(|c| c.at == ctx.at) else {
            return;
        };
        let (start, end) = (cut.start as usize, (cut.end as usize).min(layers.prims.len()));
        if end <= start {
            return;
        }
        let generation = ui::GENERATION.load(Ordering::Relaxed);
        if self.uploaded != Some(generation) {
            let count = layers.prims.len() as u64;
            if count > self.capacity {
                self.capacity = count.next_power_of_two();
                self.instances = instance_buffer(&self.device, self.capacity);
            }
            let floats: Vec<f32> = layers.prims.iter().flat_map(|p| p.floats()).collect();
            ctx.queue.write_buffer(&self.instances, 0, &bytes(&floats));
            self.uploaded = Some(generation);
        }
        let wants_blur = layers.prims[start..end].iter().any(|p| p.params[2] > 0.0);
        let blurred = wants_blur && self.blur_behind(ctx);
        if wants_blur {
            ui::BLURS.store(blurred, Ordering::Relaxed);
        }
        let size = [ctx.size[0] as f32, ctx.size[1] as f32, ctx.scale];
        let (globals, group) = match &self.blur {
            Some(blur) if blurred => (&blur.globals, &blur.group),
            _ => (&self.globals, &self.group),
        };
        ctx.queue.write_buffer(globals, 0, &bytes(&[size[0], size[1], size[2], f32::from(u8::from(blurred)), 0.0, 0.0, 0.0, 0.0]));
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: ctx.target,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
        })];
        let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Sunset UI shapes"),
            color_attachments: &attachments,
            ..Default::default()
        });
        pass.set_pipeline(&self.shapes);
        pass.set_bind_group(0, group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..4, start as u32..end as u32);
    }
}

fn group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, globals: &wgpu::Buffer, view: &wgpu::TextureView) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Sunset UI"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

fn instance_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Sunset UI shapes"),
        size: capacity * STRIDE,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

// The fork's hud_cut calls cut at each place set_hud_cuts named (installed once; called every frame after that).
pub fn register() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        horizon_client::gfx::hud_cut(cut);
    });
}

// A frame begins: the last one's shapes and cuts are gone, so a frame that builds none (the title screen) draws none.
pub fn begin_frame() {
    register();
    if ui::begin_frame() {
        horizon_client::gfx::set_hud_cuts(&[]);
    }
}

// The frame's HUD is built up to where the menu ends: the renderer learns where the layers go.
pub fn end_frame() {
    let cuts = ui::frame(|layers| {
        layers.seal();
        layers.cut_ats()
    });
    if !cuts.is_empty() {
        horizon_client::gfx::set_hud_cuts(&cuts);
    }
}

fn cut(ctx: &mut HudCut<'_>) {
    if !::sunset_core::lifecycle::RUNTIME.running() {
        return;
    }
    STATE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.as_ref().is_some_and(|s| s.device != *ctx.device || s.format != ctx.format) {
            *slot = None;
        }
        // A layered frame's HUD also writes its coverage, which these passes don't: the HUD's triangles draw there.
        if ctx.coverage || FAILED.get() {
            ui::SUPPORTED.store(false, Ordering::Relaxed);
            return;
        }
        if slot.is_none() {
            *slot = State::new(ctx.device, ctx.format);
            if slot.is_none() {
                FAILED.set(true);
                ui::SUPPORTED.store(false, Ordering::Relaxed);
                return;
            }
            log::info!("Sunset UI pipeline ready ({:?})", ctx.format);
        }
        ui::SUPPORTED.store(true, Ordering::Relaxed);
        if let Some(state) = slot.as_mut() {
            ui::frame(|layers| state.draw(ctx, layers));
        }
    });
}

// Before the renderer goes (or is replaced): nothing of the device may outlive it.
pub fn release() {
    let _ = STATE.try_with(|cell| *cell.borrow_mut() = None);
    ui::SUPPORTED.store(false, Ordering::Relaxed);
}

use std::{fs, io::Write, path::Path, sync::mpsc};

const WIDTH: u32 = 2000;
const HEIGHT: u32 = 1125;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

fn mapped(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
    let (send, recv) = mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |result| send.send(result).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    recv.recv().unwrap().unwrap();
    let data = buffer.get_mapped_range(..).unwrap().to_vec();
    buffer.unmap();
    data
}

struct Rig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target: wgpu::Texture,
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    group: wgpu::BindGroup,
    layout: wgpu::PipelineLayout,
}

impl Rig {
    fn pipeline(&self, path: &Path, mode: u32, quality: u32, native: bool) -> wgpu::RenderPipeline {
        let text = fs::read_to_string(path).unwrap();
        let module = naga::front::wgsl::parse_str(&text).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&text)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default()).validate(&module).unwrap();
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: path.to_str(), source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
        });
        let constants = [
            ("SUNSET_MODE", mode as f64), ("SUNSET_QUALITY", quality as f64),
            ("SUNSET_BRIGHTNESS", 1.0), ("SUNSET_STRENGTH", 1.0),
            ("SUNSET_STARS", 1.0), ("SUNSET_SPEED", 0.0), ("SUNSET_DAY_BLEND", 0.65),
        ];
        self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: path.to_str(), layout: Some(&self.layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_sky"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: Some(false), depth_compare: Some(wgpu::CompareFunction::Always), stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some(if native { "fs_sky" } else { "fs_sunset_atmosphere" }), compilation_options: wgpu::PipelineCompilationOptions { constants: if native { &[] } else { &constants }, ..Default::default() }, targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            multiview_mask: None, cache: None,
        })
    }

    fn draw(&self, encoder: &mut wgpu::CommandEncoder, pipeline: &wgpu::RenderPipeline, writes: Option<wgpu::RenderPassTimestampWrites<'_>>) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Full native resolution sky triangle only"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &self.color, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &self.depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
            timestamp_writes: writes, ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn warm(&self, pipeline: &wgpu::RenderPipeline, count: usize) {
        let mut encoder = self.device.create_command_encoder(&Default::default());
        for _ in 0..count { self.draw(&mut encoder, pipeline, None); }
        self.queue.submit([encoder.finish()]);
        self.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    }

    fn warm_pair(&self, old: &wgpu::RenderPipeline, new: &wgpu::RenderPipeline) -> f64 {
        let started = std::time::Instant::now();
        while started.elapsed() < std::time::Duration::from_millis(600) {
            let mut encoder = self.device.create_command_encoder(&Default::default());
            for _ in 0..32 {
                self.draw(&mut encoder, old, None);
                self.draw(&mut encoder, new, None);
            }
            self.queue.submit([encoder.finish()]);
            self.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        }
        started.elapsed().as_secs_f64() * 1000.0
    }

    fn measure(&self, pipeline: &wgpu::RenderPipeline, count: u32) -> Vec<f64> {
        let queries = self.device.create_query_set(&wgpu::QuerySetDescriptor { label: None, ty: wgpu::QueryType::Timestamp, count: count * 2 });
        let size = count as u64 * 16;
        let resolve = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let read = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        for i in 0..count {
            self.draw(&mut encoder, pipeline, Some(wgpu::RenderPassTimestampWrites { query_set: &queries, beginning_of_pass_write_index: Some(i * 2), end_of_pass_write_index: Some(i * 2 + 1) }));
        }
        encoder.resolve_query_set(&queries, 0..count * 2, &resolve, 0);
        encoder.copy_buffer_to_buffer(&resolve, 0, &read, 0, size);
        self.queue.submit([encoder.finish()]);
        let bytes = mapped(&self.device, &read);
        let period = self.queue.get_timestamp_period() as f64;
        bytes.chunks_exact(16).map(|pair| {
            let first = u64::from_le_bytes(pair[..8].try_into().unwrap());
            let last = u64::from_le_bytes(pair[8..].try_into().unwrap());
            assert!(last > first, "invalid or empty timestamp interval");
            (last - first) as f64 * period / 1_000_000.0
        }).collect()
    }

    fn capture(&self, pipeline: &wgpu::RenderPipeline, file: &Path) {
        let pitch = (WIDTH * 4).div_ceil(256) * 256;
        let read = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: pitch as u64 * HEIGHT as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.draw(&mut encoder, pipeline, None);
        encoder.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo { texture: &self.target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, wgpu::TexelCopyBufferInfo { buffer: &read, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(pitch), rows_per_image: Some(HEIGHT) } }, wgpu::Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 });
        self.queue.submit([encoder.finish()]);
        let raw = mapped(&self.device, &read);
        let mut file = fs::File::create(file).unwrap();
        for row in raw.chunks_exact(pitch as usize) { file.write_all(&row[..WIDTH as usize * 4]).unwrap(); }
    }
}

fn nvidia(path: &Path, label: &str) {
    let result = std::process::Command::new("nvidia-smi").args(["--query-gpu=timestamp,name,driver_version,pstate,clocks.current.graphics,clocks.current.memory,temperature.gpu,power.draw,power.limit,utilization.gpu", "--format=csv,noheader,nounits"]).output();
    let mut file = fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
    match result { Ok(result) => { writeln!(file, "{label}: {}", String::from_utf8_lossy(&result.stdout).trim()).unwrap(); }, Err(error) => { writeln!(file, "{label}: unavailable: {error}").unwrap(); } }
}

fn main() {
    let root = std::env::args().nth(1).expect("input/output root");
    let root = Path::new(&root);
    assert!(root.join("GO_GPU_RENDER").is_file(), "Explicit root permission gate not present; no device creation");
    let rounds: usize = std::env::var("BENCH_ROUNDS").unwrap_or("8".into()).parse().unwrap();
    let samples: u32 = std::env::var("BENCH_SAMPLES").unwrap_or("64".into()).parse().unwrap();
    nvidia(&root.join("nvidia-smi.txt"), "before-device");
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, force_fallback_adapter: false, compatible_surface: None, ..Default::default() })).unwrap();
    assert!(adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY));
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_features: wgpu::Features::TIMESTAMP_QUERY, ..Default::default() })).unwrap();
    fs::write(root.join("adapter.txt"), format!("{:?}\ntimestamp_period_ns={}\nformat={FORMAT:?}\nsize={WIDTH}x{HEIGHT}\nrounds={rounds}\nsamples_per_variant_per_round={samples}\n", adapter.get_info(), queue.get_timestamp_period())).unwrap();
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }] });
    let globals = fs::read(root.join("globals.bin")).unwrap();
    assert_eq!(globals.len(), 1088);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: globals.len() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    queue.write_buffer(&buffer, 0, &globals);
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bgl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }] });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl)], ..Default::default() });
    let descriptor = |format, usage| wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] };
    let target = device.create_texture(&descriptor(FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC));
    let depth_texture = device.create_texture(&descriptor(wgpu::TextureFormat::Depth32Float, wgpu::TextureUsages::RENDER_ATTACHMENT));
    let color = target.create_view(&Default::default());
    let depth = depth_texture.create_view(&Default::default());
    let rig = Rig { device, queue, target, color, depth, group, layout };
    let native = rig.pipeline(&root.join("native.wgsl"), 0, 0, true);
    let mut cases = Vec::new();
    for quality in [1u32, 0u32] { for mode in 0..3 {
        let label = format!("{}-{}", ["space", "aurora", "planet"][mode as usize], if quality == 1 { "balanced" } else { "performance" });
        let old = rig.pipeline(&root.join("original.wgsl"), mode, quality, false);
        let new = rig.pipeline(&root.join("optimized.wgsl"), mode, quality, false);
        cases.push((label, old, new));
    } }
    // Warm every compiled specialization, then run the expensive baseline long
    // enough to establish active clocks before collecting any timestamp samples.
    for (_, old, new) in &cases { rig.warm(old, 32); rig.warm(new, 32); }
    rig.warm(&cases[0].1, 256);
    let mut csv = fs::File::create(root.join("gpu-samples.csv")).unwrap();
    let mut warm_csv = fs::File::create(root.join("warm-blocks.csv")).unwrap();
    writeln!(csv, "case,variant,round,position,sample,gpu_ms").unwrap();
    writeln!(warm_csv, "case,variant,round,wall_ms_of_sustained_alternating_gpu_warm").unwrap();
    for (sample, value) in rig.measure(&native, samples).into_iter().enumerate() { writeln!(csv, "native,native,-1,0,{sample},{value:.9}").unwrap(); }
    for round in 0..rounds {
        nvidia(&root.join("nvidia-smi.txt"), &format!("round-{round}-start"));
        for offset in 0..cases.len() {
            let case = (offset + round) % cases.len();
            let (label, old, new) = &cases[case];
            let pair = if (round + offset) % 2 == 0 { [("original", old), ("optimized", new)] } else { [("optimized", new), ("original", old)] };
            for (position, (variant, pipeline)) in pair.into_iter().enumerate() {
                let warm_ms = rig.warm_pair(old, new);
                writeln!(warm_csv, "{label},{variant},{round},{warm_ms:.3}").unwrap();
                for (sample, value) in rig.measure(pipeline, samples).into_iter().enumerate() { writeln!(csv, "{label},{variant},{round},{position},{sample},{value:.9}").unwrap(); }
                csv.flush().unwrap();
            }
        }
        println!("Completed paired round {}/{}", round + 1, rounds);
    }
    for (sample, value) in rig.measure(&native, samples).into_iter().enumerate() { writeln!(csv, "native,native,{},0,{sample},{value:.9}", rounds).unwrap(); }
    nvidia(&root.join("nvidia-smi.txt"), "after-timing-before-capture");
    for (label, old, new) in &cases {
        rig.capture(old, &root.join(format!("{label}-original.bgra")));
        rig.capture(new, &root.join(format!("{label}-optimized.bgra")));
    }
    println!("Completed shader-only timestamp benchmark and original GPU readbacks");
}

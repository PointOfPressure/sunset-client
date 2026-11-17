// Baritone's lines, handed over by the game side once a tick and added to the lines drawn over
// everything. `camera_relative` is called three times a frame (the depth-tested lines, the lines
// over everything, the crosshair axes): on the second call this takes over and converts the game's
// lines and ours together, the same way `camera_relative` does.
use super::Line;
use std::{ops::ControlFlow, sync::Mutex, time::Instant};

static LINES: Mutex<(Vec<Line>, Vec<Line>)> = Mutex::new((Vec::new(), Vec::new()));
static CALLS: Mutex<(Option<Instant>, u32)> = Mutex::new((None, 0));

/// The depth-tested lines, and the lines drawn over everything (`ignoreDepth` of the renderer's settings).
pub fn set(depth: Vec<Line>, on_top: Vec<Line>) {
    if let Ok(mut held) = LINES.lock() {
        *held = (depth, on_top);
    }
}

pub fn take_over(lines: &[Line], camera: [f64; 3], view: glam::Mat4) -> ControlFlow<Vec<[f32; 8]>> {
    let which = match CALLS.lock() {
        Ok(mut c) => {
            let now = Instant::now();
            if c.0.is_none_or(|t| now.duration_since(t).as_millis() > 100) {
                c.1 = 0;
            }
            c.0 = Some(now);
            c.1 += 1;
            (c.1 - 1) % 3
        }
        Err(_) => 2,
    };
    // The first call is the depth-tested lines, the second the lines over everything, the third the crosshair axes.
    if which > 1 {
        return ControlFlow::Continue(());
    }
    let mine = LINES.lock().map(|l| if which == 0 { l.0.clone() } else { l.1.clone() }).unwrap_or_default();
    if mine.is_empty() {
        return ControlFlow::Continue(());
    }
    let mut out = Vec::with_capacity(lines.len() + mine.len());
    for l in lines.iter().chain(mine.iter()) {
        let rel = |p: [f64; 3]| {
            glam::Vec3::new(
                (p[0] - camera[0]) as f32,
                (p[1] - camera[1]) as f32,
                (p[2] - camera[2]) as f32,
            )
        };
        let (mut start, mut end) = (rel(l.start), rel(l.end));
        let (sv, ev) = (view.transform_point3(start), view.transform_point3(end));
        let (start_behind, end_behind) = (sv.z > -0.05, ev.z > -0.05);
        if start_behind && end_behind {
            continue;
        }
        if start_behind || end_behind {
            let denom = ev.z - sv.z;
            if denom.abs() >= 1.0e-9 {
                let t = ((-0.05 - sv.z) / denom).clamp(0.0, 1.0);
                let cut = start.lerp(end, t);
                if start_behind {
                    start = cut;
                } else {
                    end = cut;
                }
            }
        }
        out.push([start.x, start.y, start.z, end.x, end.y, end.z, f32::from_bits(l.color), l.width]);
    }
    ControlFlow::Break(out)
}

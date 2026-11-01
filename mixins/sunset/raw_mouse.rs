// Frame-local native device counts before sensitivity and camera smoothing.
// This support module has no Game, renderer, packet or window dependency.
static MOTION:std::sync::Mutex<[f64;2]>=std::sync::Mutex::new([0.0;2]);
pub fn note(delta:[f64;2],active:bool,grabbed:bool) {
    *MOTION.lock().unwrap_or_else(|e|e.into_inner())=
        if active&&grabbed&&delta.iter().all(|v|v.is_finite()) {delta}else{[0.0;2]};
}
pub fn take()->[f64;2] {
    std::mem::replace(&mut *MOTION.lock().unwrap_or_else(|e|e.into_inner()),[0.0;2])
}

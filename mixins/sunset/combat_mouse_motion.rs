// Observe accepted native counts before sensitivity/cinematic smoothing.
fn sunset_combat_mouse_motion(this:&MouseLook,active:bool,grabbed:bool) {
    let (x,y)=this.accumulated();
    sunset_mouse_input::note([x,y],active,grabbed);
}

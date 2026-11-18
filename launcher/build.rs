// The launcher's icon on Windows: assets/sunset-launcher.rc puts assets/sunset-launcher.ico in as icon resource 1,
// which Explorer shows for the executable and gpui loads for its windows (title bar, taskbar, Alt+Tab).
// embed-resource runs rc.exe for MSVC targets and windres for windows-gnu (the release-cross build from Linux).
fn main() {
    println!("cargo:rerun-if-changed=assets/sunset-launcher.rc");
    println!("cargo:rerun-if-changed=assets/sunset-launcher.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/sunset-launcher.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("the launcher's icon resource (assets/sunset-launcher.rc) did not compile");
    }
}

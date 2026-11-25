//! Sunset launcher. Without arguments it opens the window; the subcommands do the same work in a terminal
//! (for scripts and headless checks).

// A window app on Windows, so opening it shows no console window beside it; the subcommands attach to the terminal
// they were started from (attach_console).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod art;
mod core;
mod fx;
mod nick_field;
mod plugins;
mod selfupdate;
mod ui;

use std::sync::atomic::AtomicBool;

const HELP: &str = "Sunset launcher

Usage: sunset-launcher [COMMAND]

Without a command, opens the window.
  check            Fetch and verify the signed manifest, and say what is installed
  self-update      Update the launcher itself to the release's build (the window does this when it opens)
  install          Install Sunset (or update it to the manifest's pin)
  repair           Check and download Sunset again, then unpack it again
  deps             Linux: the libraries horizon-launcher is missing, and how to install them
  sign-in          Sign in with Microsoft (prints the link and the code)
  play             Play the sunset instance
  offline NICK     Play the sunset instance offline (cracked) as NICK, for joining offline-mode servers
  import [NAME]    Copy settings, keybinds, servers, worlds, packs and Sunset configs from a Horizon
                   instance (files it replaces go to a backup folder; close the game first)
  plugins          The installed plugins and what the instance plays
  plugins on|off   Build plugins into Sunset, or go back to Sunset as released
  plugins add PATH Install a plugin (its folder or .hmod file), then build when plugins are on
  plugins remove ID  Remove a plugin, then build again (or go back to Sunset as released)
  plugins build    Build (or go back) now if the instance doesn't play what is asked for
  licences         Print the licences: Sunset's, the Horizon notice and the launcher's third-party licences

Environment: SUNSET_LAUNCHER_HOME (data folder), SUNSET_MANIFEST (manifest address: https://, http:// or file://),
SUNSET_NO_SELF_UPDATE (the window leaves the launcher as it is)";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    selfupdate::remove_replaced();
    let Some(cmd) = args.first() else {
        ui::run();
        return;
    };
    #[cfg(windows)]
    attach_console();
    let p = core::Paths::new();
    let s = core::Settings::load(&p);
    let since = std::time::SystemTime::now();
    let progress = |msg: &str, f: Option<f32>| match f {
        Some(f) => eprintln!("{msg} ({:.0}%)", f * 100.),
        None => eprintln!("{msg}"),
    };
    let r: core::Result<String> = match cmd.as_str() {
        "check" => core::fetch_signed(&s).and_then(|sg| {
            let launcher = match selfupdate::offered(&sg.raw, core::VERSION) {
                Ok(Some(pin)) => format!("{} (the release has {}: sunset-launcher self-update)", core::VERSION, pin.version),
                Ok(None) => format!("{}, up to date", core::VERSION),
                Err(e) => format!("{} ({e})", core::VERSION),
            };
            let r = core::Release::from_signed(sg).map_err(|e| format!("{e}\nlauncher: {launcher}"))?;
            let m = &r.manifest;
            let inst = core::installed(&p).map(|i| format!("Sunset {} ({})", i.version, i.version_dir)).unwrap_or_else(|| "nothing".into());
            Ok(format!(
                "manifest: Sunset {} on Horizon {} ({}), Minecraft {}, horizon-launcher {}\ninstalled: {inst}\nlauncher: {launcher}\ndata: {}",
                m.version, m.short_pin(), m.horizon.tag, m.horizon.game, m.cli.version, p.root.display()
            ))
        }),
        "self-update" => core::fetch_signed(&s).and_then(|sg| match selfupdate::offered(&sg.raw, core::VERSION)? {
            None => Ok(format!("the launcher is up to date ({})", core::VERSION)),
            Some(pin) => selfupdate::apply(&p, &sg.base, &pin, interrupt_cancels(), &progress)
                .map(|exe| format!("updated the launcher to {} ({}): start it again", pin.version, exe.display())),
        }),
        "install" | "repair" => core::fetch_release(&s).and_then(|r| core::install(&p, &r, cmd == "repair", interrupt_cancels(), &progress)).map(|_| "done".into()),
        "deps" => match core::cli_path(&p) {
            None => Err("horizon-launcher is not installed yet (install fetches it)".into()),
            Some(exe) => {
                let m = core::missing_libs(&exe);
                if m.is_empty() { Ok("horizon-launcher has every library it needs".into()) } else { Err(format!("missing {}\n{}", m.join(", "), core::install_hint(&m))) }
            }
        },
        "sign-in" => {
            let (tx, rx) = std::sync::mpsc::channel();
            match core::sign_in(&p, move |e| {
                let _ = tx.send(e);
            }) {
                Err(e) => Err(e),
                Ok(_child) => loop {
                    match rx.recv() {
                        Ok(core::SignIn::Code { link, code }) => println!("Open {link} and enter the code {code}"),
                        Ok(core::SignIn::Done(r)) => break r.map(|n| format!("Signed in as {n}")),
                        Err(_) => break Err("sign-in stopped".into()),
                    }
                },
            }
        }
        "play" => core::play(&p, &s).and_then(|mut c| {
            use std::io::Read;
            if core::sounds_missing(&p) {
                eprintln!("sunset-launcher: {}", core::NO_SOUNDS);
            }
            let mut err = String::new();
            if let Some(mut e) = c.stderr.take() {
                let _ = e.read_to_string(&mut err);
            }
            let st = c.wait().map_err(|e| e.to_string())?;
            if st.success() {
                return Ok("the game exited".into());
            }
            let last = err.trim().lines().last().unwrap_or("");
            // The window has a Sign in button for this; here it is our command.
            Err(match core::sign_in_refusal(last) {
                Some(why) => format!("{why} Run `sunset-launcher sign-in`."),
                None => core::game_failure(&p, since, last),
            })
        }),
        "offline" => match args.get(1) {
            None => Err("usage: sunset-launcher offline NICK".into()),
            Some(_) if !core::owner_ever(&p) => Err(
                "Offline play needs a Microsoft account that owns Minecraft: Java Edition to have signed in at least once. Sign in with `sunset-launcher sign-in`.".into(),
            ),
            Some(nick) => core::play_offline(&p, &s, nick).and_then(|mut c| {
                use std::io::Read;
                if core::sounds_missing(&p) {
                    eprintln!("sunset-launcher: {}", core::NO_SOUNDS);
                }
                let mut err = String::new();
                if let Some(mut e) = c.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                let st = c.wait().map_err(|e| e.to_string())?;
                if st.success() { Ok("the game exited".into()) } else { Err(core::game_failure(&p, since, err.trim().lines().last().unwrap_or(""))) }
            }),
        },
        "import" => {
            let src = core::import_sources(&p);
            let pick = match args.get(1) {
                Some(n) => src.iter().find(|(name, _)| name.eq_ignore_ascii_case(n)),
                None => src.first(),
            };
            match pick {
                None => Err(format!("no Horizon instance{}", args.get(1).map(|n| format!(" named {n}")).unwrap_or_default())),
                Some((name, dir)) => core::import_from(&p, dir).map(|i| {
                    let mut s = s.clone();
                    s.import_decided = true;
                    let _ = s.save(&p);
                    i.note(name)
                }),
            }
        }
        "plugins" => plugins_cmd(&p, s, &args[1..], &progress),
        "licences" | "licenses" | "--licences" | "--licenses" => Ok(core::licences()),
        "-h" | "--help" | "help" => Ok(HELP.into()),
        "-V" | "--version" => Ok(format!("sunset-launcher {}", core::VERSION)),
        other => Err(format!("unknown command {other:?}\n\n{HELP}")),
    };
    match r {
        // Not println!: a closed pipe (`sunset-launcher licences | head`) is not an error worth a panic.
        Ok(out) => {
            use std::io::Write;
            let _ = writeln!(std::io::stdout().lock(), "{out}");
        }
        Err(e) => {
            eprintln!("sunset-launcher: {e}");
            std::process::exit(1);
        }
    }
}

/// The launcher is a window app on Windows, which starts with no console: a subcommand writes to the terminal it was
/// started from by attaching to that terminal's console. Output already redirected (a pipe, a file, Git Bash) keeps
/// going where it goes. cmd.exe and PowerShell don't wait for a window app: `start /wait`, or a pipe, makes them wait.
#[cfg(windows)]
fn attach_console() {
    type Handle = *mut std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process: u32) -> i32;
        fn GetStdHandle(which: u32) -> Handle;
        fn SetStdHandle(which: u32, handle: Handle) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    // STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE
    const STD: [u32; 3] = [-10i32 as u32, -11i32 as u32, -12i32 as u32];
    // SAFETY: plain Win32 calls; the handles are only passed back to SetStdHandle.
    unsafe {
        let before = STD.map(|n| GetStdHandle(n));
        // Started from Explorer or with every stream redirected: no console to attach to, or no need.
        if before.iter().all(|h| !h.is_null() && *h as isize != -1) || AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        // Attaching points every stream at the console: the redirected ones go back to where they went.
        for (n, h) in STD.iter().zip(before) {
            if !h.is_null() && h as isize != -1 {
                SetStdHandle(*n, h);
            }
        }
    }
}

/// A flag that Ctrl+C (or SIGTERM) sets, for the CLI's install and plugin build to cancel on (their steps run in
/// their own process groups, so the terminal's signal would miss them); a second Ctrl+C quits at once.
/// On Windows the build's job ends with the launcher already.
fn interrupt_cancels() -> &'static AtomicBool {
    static CANCEL: AtomicBool = AtomicBool::new(false);
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn signal(sig: i32, handler: usize) -> usize;
        }
        extern "C" fn on_signal(sig: i32) {
            CANCEL.store(true, std::sync::atomic::Ordering::Relaxed);
            // SAFETY: signal() is async-signal-safe; back to the default action for the next one.
            unsafe { signal(sig, 0) };
        }
        const SIGINT: i32 = 2;
        const SIGTERM: i32 = 15;
        // SAFETY: the handler only stores to an atomic and resets itself.
        unsafe {
            signal(SIGINT, on_signal as extern "C" fn(i32) as usize);
            signal(SIGTERM, on_signal as extern "C" fn(i32) as usize);
        }
    }
    &CANCEL
}

fn plugins_cmd(p: &core::Paths, mut s: core::Settings, args: &[String], progress: core::Progress) -> core::Result<String> {
    let m = core::installed(p).ok_or("Sunset is not installed yet (install first)")?;
    match args.first().map(String::as_str) {
        None => {
            let list = plugins::list(p);
            let mut out: Vec<String> = list.iter().map(|pl| format!("{} {} ({})", pl.id, pl.version, pl.name)).collect();
            if out.is_empty() {
                out.push("no plugins installed".into());
            }
            let state = match plugins::status(p, &m, s.plugins_enabled, &list) {
                plugins::Status::Clean => "the instance plays Sunset as released".to_string(),
                plugins::Status::Built(b) => format!("the instance plays Sunset with {} plugin(s) ({})", b.plugins.len(), b.version_dir),
                plugins::Status::NeedsBuild => "the instance needs a plugin build (sunset-launcher plugins build)".into(),
                plugins::Status::NeedsClean => "the instance still plays a plugin build (sunset-launcher plugins build goes back)".into(),
            };
            out.push(format!("plugins {}; {state}", if s.plugins_enabled { "on" } else { "off" }));
            return Ok(out.join("
"));
        }
        Some("on") | Some("off") => {
            s.plugins_enabled = args[0] == "on";
            s.save(p)?;
        }
        Some("add") => {
            let path = args.get(1).ok_or("plugins add PATH")?;
            let pl = plugins::add(p, std::path::Path::new(path), &m)?;
            eprintln!("installed {} {}", pl.id, pl.version);
        }
        Some("remove") => plugins::remove(p, args.get(1).ok_or("plugins remove ID")?)?,
        Some("build") => {}
        Some(other) => return Err(format!("unknown plugins command {other:?}

{HELP}")),
    }
    // The compile's clock ticks four times a second: a line is printed once.
    let last = std::cell::RefCell::new(String::new());
    let report = |phase: plugins::Phase, msg: &str, f: Option<f32>| {
        let line = format!("{}: {msg}", phase.label());
        if *last.borrow() != line {
            progress(&line, f.map(|f| phase.overall(f)));
            *last.borrow_mut() = line;
        }
    };
    // Ctrl+C cancels the build: native-build runs in its own process group, so the terminal's signal would miss it.
    let cancel = interrupt_cancels();
    match plugins::sync(p, &m, s.plugins_enabled, cancel, &report) {
        Ok(plugins::Synced::Unchanged) => Ok("the instance already plays that".into()),
        Ok(plugins::Synced::Cleaned) => Ok("the instance plays Sunset as released".into()),
        Ok(plugins::Synced::Built(b)) => Ok(format!("built {} with {}", b.version_dir, b.plugins.iter().map(|b| b.id.as_str()).collect::<Vec<_>>().join(", "))),
        Err(f) => Err(format!("{} failed: {}
(whole output: {})", f.phase.label(), f.error, plugins::log_path(p).display())),
    }
}

//! Sunset launcher. Without arguments it opens the window; the subcommands do the same work in a terminal
//! (for scripts and headless checks).

mod core;
mod fx;
mod plugins;
mod ui;

use std::sync::atomic::AtomicBool;

const HELP: &str = "Sunset launcher

Usage: sunset-launcher [COMMAND]

Without a command, opens the window.
  check            Fetch and verify the signed manifest, and say what is installed
  install          Install Sunset (or update it to the manifest's pin)
  repair           Check and download Sunset again, then unpack it again
  deps             Linux: the libraries horizon-launcher is missing, and how to install them
  sign-in          Sign in with Microsoft (prints the link and the code)
  play             Play the sunset instance
  import [NAME]    Copy Sunset configs and profiles from a Horizon instance (keeps files already here)
  plugins          The installed plugins and what the instance plays
  plugins on|off   Build plugins into Sunset, or go back to Sunset as released
  plugins add PATH Install a plugin (its folder or .hmod file), then build when plugins are on
  plugins remove ID  Remove a plugin, then build again (or go back to Sunset as released)
  plugins build    Build (or go back) now if the instance doesn't play what is asked for

Environment: SUNSET_LAUNCHER_HOME (data folder), SUNSET_MANIFEST (manifest address: https://, http:// or file://)";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        ui::run();
        return;
    };
    let p = core::Paths::new();
    let s = core::Settings::load(&p);
    let progress = |msg: &str, f: Option<f32>| match f {
        Some(f) => eprintln!("{msg} ({:.0}%)", f * 100.),
        None => eprintln!("{msg}"),
    };
    let r: core::Result<String> = match cmd.as_str() {
        "check" => core::fetch_release(&s).map(|r| {
            let m = &r.manifest;
            let inst = core::installed(&p).map(|i| format!("Sunset {} ({})", i.version, i.version_dir)).unwrap_or_else(|| "nothing".into());
            format!(
                "manifest: Sunset {} on Horizon {} ({}), Minecraft {}, horizon-launcher {}\ninstalled: {inst}\ndata: {}",
                m.version, m.short_pin(), m.horizon.tag, m.horizon.game, m.cli.version, p.root.display()
            )
        }),
        "install" | "repair" => core::fetch_release(&s).and_then(|r| core::install(&p, &r, cmd == "repair", &AtomicBool::new(false), &progress)).map(|_| "done".into()),
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
            let mut err = String::new();
            if let Some(mut e) = c.stderr.take() {
                let _ = e.read_to_string(&mut err);
            }
            let st = c.wait().map_err(|e| e.to_string())?;
            if st.success() { Ok("the game exited".into()) } else { Err(err.trim().lines().last().unwrap_or("play failed").to_string()) }
        }),
        "import" => {
            let src = core::import_sources(&p);
            let pick = match args.get(1) {
                Some(n) => src.iter().find(|(name, _)| name == n),
                None => src.first(),
            };
            match pick {
                None => Err(format!("no Horizon instance with Sunset configs{}", args.get(1).map(|n| format!(" named {n}")).unwrap_or_default())),
                Some((name, dir)) => core::import_from(&p, dir, false).map(|n| {
                    let mut s = s.clone();
                    s.import_decided = true;
                    let _ = s.save(&p);
                    format!("imported {n} files from {name}")
                }),
            }
        }
        "plugins" => plugins_cmd(&p, s, &args[1..], &progress),
        "-h" | "--help" | "help" => Ok(HELP.into()),
        "-V" | "--version" => Ok(format!("sunset-launcher {}", core::VERSION)),
        other => Err(format!("unknown command {other:?}\n\n{HELP}")),
    };
    match r {
        Ok(out) => println!("{out}"),
        Err(e) => {
            eprintln!("sunset-launcher: {e}");
            std::process::exit(1);
        }
    }
}

/// A flag that Ctrl+C (or SIGTERM) sets, for the CLI's plugin build to cancel on; a second Ctrl+C quits at once.
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
                plugins::Status::Clean => "Sunset as released".to_string(),
                plugins::Status::Built(b) => format!("Sunset with {} plugin(s) ({})", b.plugins.len(), b.version_dir),
                plugins::Status::NeedsBuild => "needs a plugin build (sunset-launcher plugins build)".into(),
                plugins::Status::NeedsClean => "still a plugin build (sunset-launcher plugins build goes back)".into(),
            };
            out.push(format!("plugins {}; the instance plays {state}", if s.plugins_enabled { "on" } else { "off" }));
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

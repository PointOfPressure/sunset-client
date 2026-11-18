//! Sunset launcher. Without arguments it opens the window; the subcommands do the same work in a terminal
//! (for scripts and headless checks).

mod core;
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

//! The window. All slow work runs on threads from `core` and reports back through one channel.

use crate::core::{self, Account, Manifest, Paths, Release, Settings, SignIn};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext as _, Application, Bounds, ClickEvent, ClipboardItem, Context, ElementId, Hsla, Image, ImageFormat, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement as _, PathBuilder, Render, Rgba, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    WindowBounds, WindowOptions, canvas, div, img, point, px, rgb, size,
};
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const BG: u32 = 0x0b0b0c;
const CARD: u32 = 0x141416;
const LINE: u32 = 0x242428;
const CHIP: u32 = 0x1b1b1e;
const TEXT: u32 = 0xececee;
const MUTED: u32 = 0x8e8e95;
const ACCENT: u32 = 0xf0a575;
const ON_ACCENT: u32 = 0x1c130d;
const GREEN: u32 = 0x6fcf97;
const AMBER: u32 = 0xe7b04a;
const RED: u32 = 0xef6b6b;
const BODY: &str = "Inter Medium";
const BOLD: &str = "Inter SemiBold";

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Overview,
    Plugins,
    Profiles,
    Settings,
    Launcher,
}

#[derive(Clone, Copy, PartialEq)]
enum Menu {
    Account,
    More,
    Window,
}

enum Msg {
    Progress(String, Option<f32>),
    JobDone(core::Result<String>),
    Latest(core::Result<Release>, bool),
    Accounts(Vec<Account>),
    Face(String, Vec<[u8; 4]>),
    SignIn(SignIn),
    GameExited(Option<String>),
    Sources(Vec<(String, PathBuf)>),
    Info(core::Result<String>),
}

struct Job {
    label: String,
    frac: Option<f32>,
    cancel: Arc<AtomicBool>,
}

struct SignInView {
    link: Option<String>,
    code: Option<String>,
    copied: bool,
    error: Option<String>,
    child: Option<Arc<Mutex<Child>>>,
}

pub struct Launcher {
    paths: Paths,
    settings: Settings,
    installed: Option<Manifest>,
    latest: Option<Release>,
    latest_err: Option<String>,
    accounts: Vec<Account>,
    faces: Vec<(String, Vec<[u8; 4]>)>,
    tab: Tab,
    menu: Option<Menu>,
    job: Option<Job>,
    note: Option<(bool, String)>,
    sign_in: Option<SignInView>,
    about: bool,
    playing: bool,
    sources: Vec<(String, PathBuf)>,
    profiles: Vec<String>,
    mods: Vec<(String, String)>,
    tx: async_channel::Sender<Msg>,
    logo: Arc<Image>,
}

impl Launcher {
    fn new(cx: &mut Context<Self>) -> Self {
        let paths = Paths::new();
        let settings = Settings::load(&paths);
        let (tx, rx) = async_channel::unbounded::<Msg>();
        cx.spawn(async move |this, cx| {
            while let Ok(msg) = rx.recv().await {
                if this.update(cx, |app, cx| app.on(msg, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        let mut app = Launcher {
            installed: core::installed(&paths),
            paths,
            settings,
            latest: None,
            latest_err: None,
            accounts: vec![],
            faces: vec![],
            tab: Tab::Overview,
            menu: None,
            job: None,
            note: None,
            sign_in: None,
            about: false,
            playing: false,
            sources: vec![],
            profiles: vec![],
            mods: vec![],
            tx,
            logo: Arc::new(Image::from_bytes(ImageFormat::Png, include_bytes!("../../docs/images/icon.png").to_vec())),
        };
        app.reload_local();
        app.refresh_accounts();
        app.check(true);
        app
    }

    fn send(&self) -> async_channel::Sender<Msg> {
        self.tx.clone()
    }

    fn reload_local(&mut self) {
        self.installed = core::installed(&self.paths);
        self.profiles = core::profiles(&self.paths);
        self.mods = self.installed.as_ref().map(|m| core::built_in_mods(&self.paths, m)).unwrap_or_default();
        if self.installed.is_some() && !self.settings.import_decided {
            let (p, tx) = (self.paths.clone(), self.send());
            std::thread::spawn(move || {
                let _ = tx.send_blocking(Msg::Sources(core::import_sources(&p)));
            });
        }
    }

    fn refresh_accounts(&self) {
        if core::cli_path(&self.paths).is_none() {
            return;
        }
        let (p, tx) = (self.paths.clone(), self.send());
        std::thread::spawn(move || {
            let list = core::accounts(&p).unwrap_or_default();
            let _ = tx.send_blocking(Msg::Accounts(list.clone()));
            for a in list {
                if let Ok(face) = core::skin_face(&a.uuid) {
                    let _ = tx.send_blocking(Msg::Face(a.uuid, face));
                }
            }
        });
    }

    /// Fetches the signed manifest. `auto` (start-up) installs a newer pin when auto-update is on and Sunset is
    /// installed; a first install waits for New instance. Otherwise the result is shown.
    fn check(&self, auto: bool) {
        let (s, tx) = (self.settings.clone(), self.send());
        std::thread::spawn(move || {
            let _ = tx.send_blocking(Msg::Latest(core::fetch_release(&s), auto));
        });
    }

    fn update_available(&self) -> Option<&Manifest> {
        let (i, l) = (self.installed.as_ref()?, &self.latest.as_ref()?.manifest);
        (i.version != l.version || i.version_dir != l.version_dir).then_some(l)
    }

    fn start_install(&mut self, repair: bool, cx: &mut Context<Self>) {
        if self.job.is_some() {
            return;
        }
        let Some(rel) = self.latest.clone() else {
            self.note = Some((true, self.latest_err.clone().unwrap_or_else(|| "Still looking for the Sunset release…".into())));
            self.check(false);
            cx.notify();
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let label = if repair { "Repairing" } else { "Installing" };
        self.job = Some(Job { label: format!("{label} Sunset {}", rel.manifest.version), frac: None, cancel: cancel.clone() });
        self.note = None;
        let (p, tx) = (self.paths.clone(), self.send());
        std::thread::spawn(move || {
            let ptx = tx.clone();
            let progress = move |s: &str, f: Option<f32>| {
                let _ = ptx.send_blocking(Msg::Progress(s.to_string(), f));
            };
            let r = core::install(&p, &rel, repair, &cancel, &progress).map(|_| {
                format!("Sunset {} is {}", rel.manifest.version, if repair { "repaired" } else { "installed" })
            });
            let _ = tx.send_blocking(Msg::JobDone(r));
        });
        cx.notify();
    }

    fn start_sign_in(&mut self, cx: &mut Context<Self>) {
        self.menu = None;
        if core::cli_path(&self.paths).is_none() {
            // The CLI comes with the first install: fetch just the CLI now.
            let Some(rel) = self.latest.clone() else {
                self.note = Some((true, self.latest_err.clone().unwrap_or_else(|| "Still looking for the Sunset release…".into())));
                cx.notify();
                return;
            };
            self.sign_in = Some(SignInView { link: None, code: None, copied: false, error: None, child: None });
            let (p, tx) = (self.paths.clone(), self.send());
            std::thread::spawn(move || {
                let r = core::ensure_cli(&p, &rel.manifest.cli, &AtomicBool::new(false), &|_, _| {}).and_then(|exe| {
                    let missing = core::missing_libs(&exe);
                    if missing.is_empty() { Ok(()) } else { Err(format!("horizon-launcher needs {}: {}", missing.join(", "), core::install_hint(&missing))) }
                });
                let _ = tx.send_blocking(match r {
                    Ok(()) => Msg::Info(Ok("sign-in".into())),
                    Err(e) => Msg::SignIn(SignIn::Done(Err(e))),
                });
            });
            cx.notify();
            return;
        }
        let tx = self.send();
        match core::sign_in(&self.paths, move |e| {
            let _ = tx.send_blocking(Msg::SignIn(e));
        }) {
            Ok(child) => self.sign_in = Some(SignInView { link: None, code: None, copied: false, error: None, child: Some(child) }),
            Err(e) => self.sign_in = Some(SignInView { link: None, code: None, copied: false, error: Some(e), child: None }),
        }
        cx.notify();
    }

    fn cancel_sign_in(&mut self, cx: &mut Context<Self>) {
        if let Some(c) = self.sign_in.take().and_then(|s| s.child) {
            let _ = c.lock().unwrap().kill();
        }
        cx.notify();
    }

    fn play(&mut self, cx: &mut Context<Self>) {
        if self.playing || self.job.is_some() {
            return;
        }
        if self.accounts.is_empty() {
            self.start_sign_in(cx);
            return;
        }
        match core::play(&self.paths, &self.settings) {
            Ok(mut child) => {
                self.playing = true;
                self.note = None;
                let tx = self.send();
                let stderr = child.stderr.take();
                std::thread::spawn(move || {
                    use std::io::BufRead;
                    let mut last = String::new();
                    if let Some(e) = stderr {
                        for l in std::io::BufReader::new(e).lines().map_while(|l| l.ok()) {
                            if !l.trim().is_empty() {
                                last = l;
                            }
                        }
                    }
                    let ok = child.wait().map(|s| s.success()).unwrap_or(false);
                    let _ = tx.send_blocking(Msg::GameExited((!ok).then(|| if last.is_empty() { "The game stopped with an error".into() } else { last })));
                });
            }
            Err(e) => self.note = Some((true, e)),
        }
        cx.notify();
    }

    fn cli_action(&mut self, args: Vec<String>, done: &'static str, cx: &mut Context<Self>) {
        self.menu = None;
        let (p, tx) = (self.paths.clone(), self.send());
        std::thread::spawn(move || {
            let a: Vec<&str> = args.iter().map(String::as_str).collect();
            let _ = tx.send_blocking(Msg::Info(core::run_cli(&p, &a).map(|_| done.to_string())));
        });
        cx.notify();
    }

    fn on(&mut self, msg: Msg, cx: &mut Context<Self>) {
        match msg {
            Msg::Progress(label, frac) => {
                if let Some(j) = self.job.as_mut() {
                    j.label = label;
                    j.frac = frac;
                }
            }
            Msg::JobDone(r) => {
                let cancelled = self.job.take().is_some_and(|j| j.cancel.load(Ordering::Relaxed));
                self.note = match r {
                    Ok(s) => Some((false, s)),
                    Err(_) if cancelled => Some((false, "Cancelled".into())),
                    Err(e) => Some((true, e)),
                };
                self.reload_local();
                self.refresh_accounts();
            }
            Msg::Latest(r, auto) => match r {
                Ok(rel) => {
                    self.latest = Some(rel);
                    self.latest_err = None;
                    if self.update_available().is_some() && auto && self.settings.auto_update && !self.playing {
                        self.start_install(false, cx);
                    } else if !auto {
                        self.note = Some((false, match self.update_available() {
                            Some(m) => format!("Sunset {} is available", m.version),
                            None if self.installed.is_some() => "Sunset is up to date".into(),
                            None => format!("Sunset {} is ready to install", self.latest.as_ref().unwrap().manifest.version),
                        }));
                    }
                }
                Err(e) => {
                    if !auto {
                        self.note = Some((true, e.clone()));
                    }
                    self.latest_err = Some(e);
                }
            },
            Msg::Accounts(a) => self.accounts = a,
            Msg::Face(uuid, face) => {
                self.faces.retain(|(u, _)| u != &uuid);
                self.faces.push((uuid, face));
            }
            Msg::SignIn(e) => match e {
                SignIn::Code { link, code } => {
                    if let Some(s) = self.sign_in.as_mut() {
                        s.link = Some(link);
                        s.code = Some(code);
                    }
                }
                SignIn::Done(r) => {
                    if self.sign_in.is_none() {
                        return; // cancelled
                    }
                    match r {
                        Ok(name) => {
                            self.sign_in = None;
                            self.note = Some((false, if name.is_empty() { "Signed in".into() } else { format!("Signed in as {name}") }));
                            self.refresh_accounts();
                        }
                        Err(e) => {
                            if let Some(s) = self.sign_in.as_mut() {
                                s.error = Some(e);
                                s.child = None;
                            }
                        }
                    }
                }
            },
            Msg::GameExited(err) => {
                self.playing = false;
                if let Some(e) = err {
                    self.note = Some((true, e));
                }
                self.reload_local();
            }
            Msg::Sources(s) => self.sources = s,
            Msg::Info(r) => match r {
                Ok(s) if s == "sign-in" => {
                    self.sign_in = None;
                    self.start_sign_in(cx);
                }
                Ok(s) => {
                    self.note = Some((false, s));
                    self.refresh_accounts();
                }
                Err(e) => self.note = Some((true, e)),
            },
        }
        cx.notify();
    }

    fn set_settings(&mut self, f: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        f(&mut self.settings);
        if let Err(e) = self.settings.save(&self.paths) {
            self.note = Some((true, e));
        }
        cx.notify();
    }

    fn import(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some((name, dir)) = self.sources.get(i).cloned() else { return };
        self.note = Some(match core::import_from(&self.paths, &dir, false) {
            Ok(n) => (false, format!("Imported {n} files from {name} (files already here were kept)")),
            Err(e) => (true, e),
        });
        self.set_settings(|s| s.import_decided = true, cx);
        self.reload_local();
    }

    fn open(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.menu = None;
        let _ = std::fs::create_dir_all(&path);
        cx.open_with_system(&path);
    }
}

impl Drop for Launcher {
    fn drop(&mut self) {
        // A sign-in still waiting for the browser ends with the window; a running game keeps going.
        if let Some(c) = self.sign_in.take().and_then(|s| s.child) {
            let _ = c.lock().unwrap().kill();
        }
    }
}

// ---------------------------------------------------------------- pieces

fn text(s: impl Into<SharedString>, size_px: f32, color: u32) -> gpui::Div {
    div().text_size(px(size_px)).text_color(rgb(color)).child(s.into())
}

fn bold(s: impl Into<SharedString>, size_px: f32) -> gpui::Div {
    text(s, size_px, TEXT).font_family(BOLD)
}

fn card() -> gpui::Div {
    div().bg(rgb(CARD)).border_1().border_color(rgb(LINE)).rounded(px(12.))
}

/// A button that does something; there are no disabled ones (what can't work isn't shown).
fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, primary: bool) -> gpui::Stateful<gpui::Div> {
    let (bg, fg, hover) = if primary { (ACCENT, ON_ACCENT, 0xf5b88f) } else { (CHIP, TEXT, 0x26262a) };
    div()
        .id(id)
        .px(px(14.))
        .h(px(34.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .bg(rgb(bg))
        .when(!primary, |d| d.border_1().border_color(rgb(LINE)))
        .text_size(px(13.))
        .text_color(rgb(fg))
        .font_family(BOLD)
        .cursor_pointer()
        .hover(move |s| s.bg(rgb(hover)))
        .child(label.into())
}

#[derive(Clone, Copy)]
enum Icon {
    Play,
    Chevron,
    Folder,
    Arrow,
}

fn icon(kind: Icon, s: f32, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b: Bounds<gpui::Pixels>, _, window: &mut Window, _| {
            let (x, y, w, h) = (b.origin.x, b.origin.y, b.size.width, b.size.height);
            let c: Rgba = rgb(color);
            let mut p = match kind {
                Icon::Play => PathBuilder::fill(),
                _ => PathBuilder::stroke(px(1.6)),
            };
            match kind {
                Icon::Play => {
                    p.move_to(point(x + w * 0.2, y + h * 0.1));
                    p.line_to(point(x + w * 0.9, y + h * 0.5));
                    p.line_to(point(x + w * 0.2, y + h * 0.9));
                    p.close();
                }
                Icon::Chevron => {
                    p.move_to(point(x + w * 0.2, y + h * 0.38));
                    p.line_to(point(x + w * 0.5, y + h * 0.66));
                    p.line_to(point(x + w * 0.8, y + h * 0.38));
                }
                Icon::Arrow => {
                    p.move_to(point(x + w * 0.12, y + h * 0.5));
                    p.line_to(point(x + w * 0.88, y + h * 0.5));
                    p.move_to(point(x + w * 0.58, y + h * 0.22));
                    p.line_to(point(x + w * 0.88, y + h * 0.5));
                    p.line_to(point(x + w * 0.58, y + h * 0.78));
                }
                Icon::Folder => {
                    p.move_to(point(x + w * 0.08, y + h * 0.22));
                    p.line_to(point(x + w * 0.4, y + h * 0.22));
                    p.line_to(point(x + w * 0.5, y + h * 0.34));
                    p.line_to(point(x + w * 0.92, y + h * 0.34));
                    p.line_to(point(x + w * 0.92, y + h * 0.82));
                    p.line_to(point(x + w * 0.08, y + h * 0.82));
                    p.close();
                }
            }
            if let Ok(path) = p.build() {
                window.paint_path(path, c);
            }
        },
    )
    .w(px(s))
    .h(px(s))
    .flex_none()
}

/// The banner: a sun behind three ridges, drawn (no image files).
fn banner() -> impl IntoElement {
    canvas(
        |_, _, _| (),
        |b: Bounds<gpui::Pixels>, _, window: &mut Window, _| {
            let (x, y, w, h) = (b.origin.x, b.origin.y, b.size.width, b.size.height);
            let poly = |pts: &[(f32, f32)], color: u32, window: &mut Window| {
                let mut p = PathBuilder::fill();
                for (i, (px_, py_)) in pts.iter().enumerate() {
                    let pt = point(x + w * *px_, y + h * *py_);
                    if i == 0 { p.move_to(pt) } else { p.line_to(pt) }
                }
                p.close();
                if let Ok(path) = p.build() {
                    window.paint_path(path, rgb(color));
                }
            };
            poly(&[(0., 0.), (1., 0.), (1., 1.), (0., 1.)], 0x3a251d, window);
            let (cx_, cy, r) = (0.75, 0.38, 0.17 * h / w);
            let circle: Vec<(f32, f32)> = (0..64)
                .map(|i| {
                    let a = i as f32 / 64. * std::f32::consts::TAU;
                    (cx_ + r * a.cos(), cy + r * (w / h) * a.sin())
                })
                .collect();
            poly(&circle, ACCENT, window);
            poly(&[(0., 0.86), (0.2, 0.3), (0.42, 0.62), (0.56, 0.4), (0.8, 0.9), (0., 1.)], 0x7a4535, window);
            poly(&[(0., 1.), (0., 0.92), (0.2, 0.55), (0.45, 0.85), (0.6, 0.6), (0.82, 0.52), (1., 0.88), (1., 1.)], 0xb0664a, window);
            poly(&[(0., 1.), (0., 0.98), (0.18, 0.78), (0.4, 0.95), (0.62, 0.8), (0.86, 0.72), (1., 0.84), (1., 1.)], 0xd98a62, window);
        },
    )
    .w_full()
    .h(px(176.))
    .rounded(px(8.))
}

fn face(pixels: &[[u8; 4]], s: f32) -> impl IntoElement {
    let cell = s / 8.;
    div().flex().flex_col().w(px(s)).h(px(s)).rounded(px(3.)).overflow_hidden().children((0..8).map(|y| {
        div().flex().h(px(cell)).children((0..8).map(move |x| {
            let [r, g, b, _] = pixels[y * 8 + x];
            div().w(px(cell)).h(px(cell)).bg(rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32))
        }))
    }))
}

fn row(label: impl Into<SharedString>, value: impl IntoElement) -> gpui::Div {
    div().flex().items_center().justify_between().h(px(32.)).child(text(label, 13., MUTED)).child(value)
}

fn setting_row(label: impl Into<SharedString>, value: impl IntoElement, last: bool) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .min_h(px(50.))
        .px(px(14.))
        .when(!last, |d| d.border_b_1().border_color(rgb(LINE)))
        .child(text(label, 13., TEXT))
        .child(value)
}

fn section(title: &str, rows: Vec<gpui::Div>) -> gpui::Div {
    let n = rows.len();
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(bold(title.to_string(), 15.))
        .child(card().flex().flex_col().children(rows.into_iter().enumerate().map(move |(i, r)| r.when(i + 1 == n, |d| d.border_b_0()))))
}

fn window_label(w: &str) -> String {
    if w == "fullscreen" { "Fullscreen".into() } else { w.replace('x', " × ") }
}

// ---------------------------------------------------------------- views

impl Launcher {
    fn top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let has = self.installed.is_some();
        let tabs = [(Tab::Overview, "Overview"), (Tab::Plugins, "Plugins"), (Tab::Profiles, "Profiles"), (Tab::Settings, "Settings")];
        let chip = |label: String| {
            div().flex().items_center().h(px(36.)).px(px(14.)).rounded(px(18.)).bg(rgb(CHIP)).border_1().border_color(rgb(LINE)).child(text(label, 13., TEXT))
        };
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(chip("Horizon".into()))
            .child(chip(if has { "Sunset".into() } else { "No instance".into() }))
            .child(div().flex_1())
            .child(
                div().flex().p(px(3.)).gap(px(2.)).rounded(px(20.)).bg(rgb(CHIP)).border_1().border_color(rgb(LINE)).children(tabs.into_iter().filter(|(t, _)| has || *t == Tab::Overview).map(|(t, label)| {
                    let on = self.tab == t;
                    div()
                        .id(label)
                        .px(px(18.))
                        .h(px(30.))
                        .flex()
                        .items_center()
                        .rounded(px(15.))
                        .when(on, |d| d.bg(rgb(0x2a2a2e)))
                        .cursor_pointer()
                        .text_size(px(13.))
                        .text_color(rgb(if on { TEXT } else { MUTED }))
                        .child(label)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.tab = t;
                            this.menu = None;
                            if t == Tab::Profiles || t == Tab::Plugins {
                                this.reload_local();
                            }
                            cx.notify();
                        }))
                })),
            )
            .child(div().flex_1())
            .child(self.account_chip(cx))
    }

    fn account_chip(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(a) = self.accounts.iter().find(|a| a.selected).or(self.accounts.first()) else {
            return button("sign-in", "Sign in", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_sign_in(cx))).into_any_element();
        };
        let head = self.faces.iter().find(|(u, _)| u == &a.uuid).map(|(_, f)| face(f, 22.).into_any_element()).unwrap_or_else(|| div().w(px(22.)).h(px(22.)).rounded(px(3.)).bg(rgb(0x3a3a40)).into_any_element());
        div()
            .id("account")
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(36.))
            .pl(px(8.))
            .pr(px(10.))
            .rounded(px(18.))
            .bg(rgb(CHIP))
            .border_1()
            .border_color(rgb(LINE))
            .cursor_pointer()
            .child(head)
            .child(text(a.name.clone(), 13., TEXT))
            .child(icon(Icon::Chevron, 14., MUTED))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.menu = if this.menu == Some(Menu::Account) { None } else { Some(Menu::Account) };
                cx.notify();
            }))
            .into_any_element()
    }

    fn menu_item(&self, id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &mut Context<Self>, f: impl Fn(&mut Self, &mut Context<Self>) + 'static) -> impl IntoElement {
        div()
            .id(id)
            .px(px(12.))
            .h(px(32.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x26262a)))
            .text_size(px(13.))
            .text_color(rgb(TEXT))
            .child(label.into())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.menu = None;
                f(this, cx);
                cx.notify();
            }))
    }

    fn menu_popup(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let m = self.menu?;
        let pop = card().absolute().p(px(4.)).min_w(px(200.)).flex().flex_col().shadow_lg();
        let pop = match m {
            Menu::Account => {
                let mut pop = pop.top(px(64.)).right(px(20.));
                for (i, a) in self.accounts.iter().enumerate() {
                    let (uuid, name) = (a.uuid.clone(), a.name.clone());
                    let label = if a.selected { format!("{}  (playing)", a.name) } else { format!("Play as {}", a.name) };
                    pop = pop.child(self.menu_item(("acct", i), label, cx, move |this, cx| {
                        this.cli_action(vec!["account".into(), "select".into(), uuid.clone()], "Account selected", cx)
                    }));
                    let uuid = a.uuid.clone();
                    pop = pop.child(self.menu_item(("signout", i), format!("Sign out {name}"), cx, move |this, cx| {
                        this.cli_action(vec!["account".into(), "sign-out".into(), uuid.clone()], "Signed out", cx)
                    }));
                }
                pop.child(self.menu_item("add-account", "Add account", cx, |this, cx| this.start_sign_in(cx)))
            }
            Menu::More => {
                let logs = self.paths.logs();
                pop.top(px(400.)).left(px(330.))
                    .child(self.menu_item("m-logs", "Open logs", cx, move |this, cx| this.open(logs.clone(), cx)))
                    .child(self.menu_item("m-check", "Check for updates", cx, |this, _| this.check(false)))
                    .child(self.menu_item("m-repair", "Repair installation", cx, |this, cx| this.start_install(true, cx)))
            }
            Menu::Window => {
                let mut pop = pop.top(px(200.)).right(px(40.));
                for w in core::WINDOW_SIZES {
                    pop = pop.child(self.menu_item(SharedString::from(w), window_label(w), cx, move |this, cx| this.set_settings(|s| s.window = w.into(), cx)));
                }
                pop
            }
        };
        Some(
            div()
                .absolute()
                .size_full()
                .top_0()
                .left_0()
                .child(div().id("menu-backdrop").absolute().size_full().on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.menu = None;
                    cx.notify();
                })))
                .child(pop.occlude()),
        )
    }

    fn status(&self) -> (u32, String) {
        if self.playing {
            return (GREEN, "Playing".into());
        }
        if self.job.is_some() {
            return (AMBER, "Working…".into());
        }
        match self.update_available() {
            Some(m) => (AMBER, format!("Sunset {} available", m.version)),
            None => (GREEN, "Ready to play".into()),
        }
    }

    fn progress(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let j = self.job.as_ref()?;
        let frac = j.frac.unwrap_or(0.).clamp(0., 1.);
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .w_full()
                .child(div().flex().items_center().justify_between().child(text(j.label.clone(), 13., TEXT)).child(button("cancel-job", "Cancel", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    if let Some(j) = this.job.as_mut() {
                        j.cancel.store(true, Ordering::Relaxed);
                        j.label = "Cancelling…".into();
                    }
                    cx.notify();
                }))))
                .child(div().h(px(6.)).w_full().rounded(px(3.)).bg(rgb(0x2a2a2e)).child(div().h_full().rounded(px(3.)).bg(rgb(ACCENT)).w(gpui::relative(if j.frac.is_some() { frac } else { 1. })).when(j.frac.is_none(), |d| d.opacity(0.35)))),
        )
    }

    fn overview(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(m) = self.installed.as_ref() else { return self.no_instance(cx).into_any_element() };
        let (dot, status) = self.status();
        let update = self.update_available().map(|u| u.version.clone());
        let play_area = match self.progress(cx) {
            Some(p) => p.into_any_element(),
            None => div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .id("play")
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(10.))
                        .w(px(200.))
                        .h(px(44.))
                        .rounded(px(10.))
                        .bg(rgb(ACCENT))
                        .when(self.playing, |d| d.opacity(0.6))
                        .when(!self.playing, |d| d.cursor_pointer().hover(|s| s.bg(rgb(0xf5b88f))))
                        .child(icon(Icon::Play, 14., ON_ACCENT))
                        .child(div().text_size(px(16.)).font_family(BOLD).text_color(rgb(ON_ACCENT)).child(if self.playing { "Playing" } else { "Play" }))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.play(cx))),
                )
                .when_some(update, |d, v| d.child(button("update", format!("Update to {v}"), false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_install(false, cx)))))
                .child(button("open-folder", "Open folder", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    let p = this.paths.instance();
                    this.open(p, cx)
                })))
                .child(div().flex_1())
                .child(button("more", "…", false).w(px(40.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.menu = if this.menu == Some(Menu::More) { None } else { Some(Menu::More) };
                    cx.notify();
                })))
                .into_any_element(),
        };
        let left = card()
            .flex_1()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(banner())
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_start()
                    .child(div().flex().flex_col().gap(px(2.)).child(bold(format!("Sunset {}", m.version), 20.)).child(text(format!("Minecraft {}", m.horizon.game), 13., MUTED)))
                    .child(div().flex().items_center().gap(px(6.)).child(div().w(px(7.)).h(px(7.)).rounded_full().bg(rgb(dot))).child(text(status, 12., dot))),
            )
            .child(play_area);
        let plugins = self.mods.iter().filter(|(id, _)| id != "sunset").count();
        let right = card()
            .w(px(330.))
            .p(px(18.))
            .flex()
            .flex_col()
            .child(bold("Installation", 16.))
            .child(row("Platform", text("Horizon", 13., TEXT)))
            .child(row("Runtime", text(m.short_pin().to_string(), 13., TEXT)))
            .child(row("Sunset", text(m.version.clone(), 13., TEXT)))
            .child(div().h(px(1.)).bg(rgb(LINE)).my(px(10.)))
            .child(bold("Configuration", 16.))
            .child(row("Profiles", text(format!("{} saved", self.profiles.len()), 13., TEXT)))
            .child(row("Plugins", text(if plugins == 0 { "Built in".to_string() } else { format!("{plugins} built in") }, 13., TEXT)))
            .child(div().flex_1().min_h(px(12.)))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button("to-plugins", "Plugins", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.tab = Tab::Plugins;
                        cx.notify();
                    })))
                    .child(button("to-profiles", "Profiles", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.tab = Tab::Profiles;
                        this.reload_local();
                        cx.notify();
                    }))),
            );
        let dir = self.paths.instance();
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(div().flex().items_center().justify_between().child(bold("Sunset", 28.)).child(button("manage", "Manage instance", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.tab = Tab::Settings;
                cx.notify();
            }))))
            .when_some(self.import_prompt(cx), |d, p| d.child(p))
            .child(div().flex().gap(px(14.)).child(left).child(right))
            .child(
                card()
                    .id("instance-dir")
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .h(px(46.))
                    .px(px(16.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0x18181b)))
                    .child(icon(Icon::Folder, 16., MUTED))
                    .child(text("Instance directory", 13., TEXT))
                    .child(div().flex_1())
                    .child(text(dir.display().to_string(), 12., MUTED))
                    .child(icon(Icon::Arrow, 16., MUTED))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(dir.clone(), cx))),
            )
            .into_any_element()
    }

    fn import_prompt(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        if self.settings.import_decided || self.sources.is_empty() || self.job.is_some() {
            return None;
        }
        let (name, _) = &self.sources[0];
        Some(
            card()
                .flex()
                .items_center()
                .gap(px(10.))
                .p(px(14.))
                .child(div().flex_1().child(text(format!("Bring your Sunset configs and profiles over from the Horizon instance \"{name}\"? Files already here are kept."), 13., TEXT)))
                .child(button("import-0", "Import", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.import(0, cx))))
                .child(button("import-no", "Not now", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.set_settings(|s| s.import_decided = true, cx)))),
        )
    }

    fn no_instance(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let inner = match self.progress(cx) {
            Some(p) => div().w(px(460.)).child(p).into_any_element(),
            None => div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(12.))
                .child(bold("No instances", 22.))
                .child(text(
                    match &self.latest {
                        Some(r) => format!("Sunset {} for Minecraft {} (Horizon {})", r.manifest.version, r.manifest.horizon.game, r.manifest.short_pin()),
                        None => self.latest_err.clone().unwrap_or_else(|| "Looking for the Sunset release…".into()),
                    },
                    13.,
                    MUTED,
                ))
                .child(if self.latest.is_some() {
                    button("new-instance", "New instance", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_install(false, cx))).into_any_element()
                } else {
                    button("retry", "Try again", false).on_click(cx.listener(|this, _: &ClickEvent, _, _| this.check(false))).into_any_element()
                })
                .into_any_element(),
        };
        div().flex().flex_col().gap(px(14.)).size_full().child(bold("Sunset", 28.)).child(card().flex_1().min_h(px(300.)).flex().items_center().justify_center().child(inner))
    }

    fn settings_view(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(m) = self.installed.as_ref() else { return self.no_instance(cx).into_any_element() };
        let toggle = {
            let on = self.settings.auto_update;
            div()
                .id("auto-update")
                .w(px(38.))
                .h(px(22.))
                .rounded(px(11.))
                .p(px(3.))
                .flex()
                .when(on, |d| d.justify_end())
                .bg(rgb(if on { ACCENT } else { 0x3a3a40 }))
                .cursor_pointer()
                .child(div().w(px(16.)).h(px(16.)).rounded_full().bg(rgb(if on { ON_ACCENT } else { MUTED })))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.set_settings(|s| s.auto_update = !s.auto_update, cx)))
        };
        let version = match self.update_available() {
            Some(u) => button("s-update", format!("{} / Update to {}", m.version, u.version), false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_install(false, cx))).into_any_element(),
            None => text(m.version.clone(), 13., TEXT).into_any_element(),
        };
        let (inst, logs) = (self.paths.instance(), self.paths.logs());
        let left = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(section("Installation", vec![
                setting_row("Instance name", text("Sunset", 13., TEXT), false),
                setting_row("Sunset version", version, false),
                setting_row("Runtime", text(format!("Horizon {} ({})", m.short_pin(), m.horizon.tag), 13., TEXT), true),
            ]))
            .child(section("Updates", vec![
                setting_row("Install new Sunset releases automatically", toggle, false),
                setting_row("Check for updates", button("s-check", "Check", false).on_click(cx.listener(|this, _: &ClickEvent, _, _| this.check(false))), true),
            ]));
        let right = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(section("Launch", vec![setting_row(
                "Window size",
                button("s-window", window_label(&self.settings.window), false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.menu = if this.menu == Some(Menu::Window) { None } else { Some(Menu::Window) };
                    cx.notify();
                })),
                true,
            )]))
            .child(section("Files", vec![
                setting_row("Instance directory", button("s-open", "Open folder", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(inst.clone(), cx))), false),
                setting_row("Logs", button("s-logs", "Open logs", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(logs.clone(), cx))), false),
                setting_row("Repair installation (check and download again)", button("s-repair", "Repair", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_install(true, cx))), true),
            ]));
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(bold("Instance settings", 28.))
            .when_some(self.progress(cx), |d, p| d.child(card().p(px(14.)).child(p)))
            .child(div().flex().gap(px(16.)).child(left).child(right))
            .into_any_element()
    }

    fn list_view(&self, title: &str, empty: String, items: Vec<(String, String)>, folder: Option<PathBuf>, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let n = items.len();
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(div().flex().items_center().justify_between().child(bold(title.to_string(), 28.)).when_some(folder, |d, f| {
                d.child(button("list-folder", "Open folder", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(f.clone(), cx))))
            }))
            .child(if items.is_empty() {
                card().p(px(18.)).child(text(empty, 13., MUTED))
            } else {
                card().flex().flex_col().children(items.into_iter().enumerate().map(move |(i, (a, b))| setting_row(a, text(b, 13., MUTED), i + 1 == n)))
            })
    }

    fn launcher_view(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let root = self.paths.root.clone();
        let source = std::env::var("SUNSET_MANIFEST").ok().filter(|s| !s.is_empty()).or(self.settings.manifest_url.clone()).unwrap_or_else(|| format!("GitHub Releases ({})", core::REPO));
        let cli = core::cli_path(&self.paths).map(|p| p.parent().and_then(|d| d.file_name()).map(|v| v.to_string_lossy().into_owned()).unwrap_or_default());
        let mut import_rows: Vec<gpui::Div> = self
            .sources
            .iter()
            .enumerate()
            .map(|(i, (name, dir))| {
                setting_row(
                    format!("{name}  ({})", dir.display()),
                    button(("imp", i), "Import", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.import(i, cx))),
                    false,
                )
            })
            .collect();
        if import_rows.is_empty() {
            import_rows.push(setting_row("No Horizon instance with Sunset configs found", div(), true));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(bold("Launcher settings", 28.))
            .child(div().flex().gap(px(16.)).child(div().flex_1().flex().flex_col().gap(px(18.)).child(section("Launcher", vec![
                setting_row("Data folder", button("l-data", "Open folder", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(root.clone(), cx))), false),
                setting_row("Release source", text(source, 12., MUTED), false),
                setting_row("horizon-launcher (sign-in, install, play)", text(cli.unwrap_or_else(|| "comes with the first install".into()), 13., TEXT), true),
            ]))).child(div().flex_1().flex().flex_col().gap(px(18.)).child(section("Import Sunset configs and profiles from Horizon", import_rows))))
    }

    fn sign_in_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let s = self.sign_in.as_ref()?;
        let mut body = card().w(px(440.)).p(px(24.)).flex().flex_col().gap(px(14.)).occlude().child(bold("Sign in with Microsoft", 20.));
        if let Some(e) = &s.error {
            body = body
                .child(text(e.clone(), 13., RED))
                .child(div().flex().gap(px(8.)).child(button("si-retry", "Try again", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.sign_in = None;
                    this.start_sign_in(cx)
                }))).child(button("si-close", "Close", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_sign_in(cx)))));
        } else if let (Some(link), Some(code)) = (&s.link, &s.code) {
            let (l, c) = (link.clone(), code.clone());
            body = body
                .child(text("Open this page in your browser and enter the code. Use the Microsoft account that owns Minecraft: Java Edition.", 13., MUTED))
                .child(div().id("si-link").flex().items_center().gap(px(8.)).cursor_pointer().child(text(link.clone(), 14., ACCENT)).child(icon(Icon::Arrow, 14., ACCENT)).on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&l))))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(div().px(px(16.)).py(px(8.)).rounded(px(8.)).bg(rgb(CHIP)).border_1().border_color(rgb(LINE)).child(text(code.clone(), 26., TEXT).font_family(BOLD)))
                        .child(button("si-copy", if s.copied { "Copied" } else { "Copy code" }, true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(c.clone()));
                            if let Some(s) = this.sign_in.as_mut() {
                                s.copied = true;
                            }
                            cx.notify();
                        }))),
                )
                .child(text("Waiting for you to finish in the browser…", 12., MUTED));
        } else {
            body = body.child(text("Asking Microsoft for a sign-in code…", 13., MUTED));
        }
        body = body.child(div().flex().justify_end().child(button("si-cancel", "Cancel", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_sign_in(cx)))));
        Some(div().absolute().top_0().left_0().size_full().flex().items_center().justify_center().bg(Hsla::from(gpui::rgba(0x000000b0))).child(body))
    }

    fn about_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        if !self.about {
            return None;
        }
        let key = include_str!("../sunset.pub").lines().next().unwrap_or("").rsplit(' ').next().unwrap_or("").to_string();
        let body = card()
            .w(px(440.))
            .p(px(24.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .occlude()
            .child(bold(format!("Sunset launcher {}", core::VERSION), 20.))
            .child(text("Installs the signed prebuilt Sunset client for the Horizon Nightly it was built and tested on, and plays it through the stock horizon-launcher, which keeps your Microsoft sign-in.", 13., MUTED))
            .child(row("Release signing key", text(key, 13., TEXT)))
            .child(row("Platform", text(core::PLATFORM, 13., TEXT)))
            .child(row("Licence", text("PolyForm Strict 1.0.0", 13., TEXT)))
            .child(text("Horizon is LGPL-3.0-or-later; the exact source each prebuilt is built from is published with it. Not affiliated with Mojang or Microsoft.", 12., MUTED))
            .child(div().flex().justify_end().child(button("about-close", "Close", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.about = false;
                cx.notify();
            }))));
        Some(div().absolute().top_0().left_0().size_full().flex().items_center().justify_center().bg(Hsla::from(gpui::rgba(0x000000b0))).child(body))
    }

    fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(40.))
            .child(img(self.logo.clone()).w(px(16.)).h(px(16.)))
            .child(text(format!("Launcher {}", core::VERSION), 12., MUTED))
            .child(div().flex_1())
            .child(
                div().id("launcher-settings").cursor_pointer().child(text("Launcher settings", 12., if self.tab == Tab::Launcher { TEXT } else { MUTED })).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.tab = Tab::Launcher;
                    this.menu = None;
                    if core::cli_path(&this.paths).is_some() {
                        let (p, tx) = (this.paths.clone(), this.send());
                        std::thread::spawn(move || {
                            let _ = tx.send_blocking(Msg::Sources(core::import_sources(&p)));
                        });
                    }
                    cx.notify();
                })),
            )
            .child(
                div().id("about").w(px(20.)).h(px(20.)).rounded_full().border_1().border_color(rgb(MUTED)).flex().items_center().justify_center().cursor_pointer().child(text("i", 11., MUTED)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.about = true;
                    cx.notify();
                })),
            )
    }
}

impl Render for Launcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = match self.tab {
            Tab::Overview => self.overview(cx),
            Tab::Settings => self.settings_view(cx),
            Tab::Plugins => {
                let items = self.mods.iter().map(|(id, v)| (if id == "sunset" { "Sunset".to_string() } else { id.clone() }, v.clone())).collect();
                self.list_view("Plugins", "Nothing built into this version yet.".into(), items, None, cx).into_any_element()
            }
            Tab::Profiles => {
                let items = self.profiles.iter().map(|p| (p.clone(), String::new())).collect();
                let empty = "No saved profiles yet. Save one in game from Sunset's menu (Right Shift).".to_string();
                self.list_view("Profiles", empty, items, Some(self.paths.sunset_data().join("profiles")), cx).into_any_element()
            }
            Tab::Launcher => self.launcher_view(cx).into_any_element(),
        };
        let note = self.note.clone().map(|(err, msg)| {
            card()
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(14.))
                .py(px(10.))
                .child(div().w(px(7.)).h(px(7.)).rounded_full().bg(rgb(if err { RED } else { GREEN })))
                .child(div().flex_1().child(text(msg, 13., if err { RED } else { TEXT })))
                .child(div().id("note-close").cursor_pointer().child(text("×", 16., MUTED)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.note = None;
                    cx.notify();
                })))
        });
        div()
            .size_full()
            .relative()
            .bg(rgb(BG))
            .font_family(BODY)
            .text_color(rgb(TEXT))
            .flex()
            .flex_col()
            .px(px(20.))
            .pt(px(16.))
            .gap(px(18.))
            .child(self.top_bar(cx))
            .child(div().id("page").flex_1().overflow_y_scroll().child(page))
            .when_some(note, |d, n| d.child(n))
            .child(self.footer(cx))
            .when_some(self.menu_popup(cx), |d, m| d.child(m))
            .when_some(self.sign_in_modal(cx), |d, m| d.child(m))
            .when_some(self.about_modal(cx), |d, m| d.child(m))
    }
}

pub fn run() {
    Application::with_platform(gpui_platform::current_platform(false)).run(|cx: &mut App| {
        let fonts = vec![
            std::borrow::Cow::Borrowed(include_bytes!("../../assets/inter.ttf").as_slice()),
            std::borrow::Cow::Borrowed(include_bytes!("../../assets/inter-bold.ttf").as_slice()),
        ];
        if let Err(e) = cx.text_system().add_fonts(fonts) {
            eprintln!("sunset-launcher: fonts: {e}");
        }
        let bounds = Bounds::centered(None, size(px(1100.), px(700.)), cx);
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(980.), px(620.))),
            app_id: Some("sunset-launcher".into()),
            titlebar: Some(gpui::TitlebarOptions { title: Some("Sunset".into()), ..Default::default() }),
            ..Default::default()
        };
        if let Err(e) = cx.open_window(opts, |_, cx| cx.new(Launcher::new)) {
            eprintln!("sunset-launcher: can't open a window: {e}");
            cx.quit();
        }
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
}

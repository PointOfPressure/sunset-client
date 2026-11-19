//! The window. All slow work runs on threads from `core` and reports back through one channel.

use crate::core::{self, Account, Manifest, Paths, Release, Settings, SignIn};
use crate::fx;
use gpui::prelude::FluentBuilder;
use gpui::{
    Animation, AnimationExt as _, SpringAnimation, TextRun, ease_out_quint, quadratic,
    App, AppContext as _, Application, Bounds, ClickEvent, ClipboardItem, Context, ElementId, Hsla, InteractiveElement as _,
    IntoElement, MouseButton, MouseDownEvent, Pixels, Point, ParentElement as _, Anchor, CursorStyle, Decorations, MouseMoveEvent, MouseUpEvent, ResizeEdge, WindowControlArea, WindowDecorations, PathBuilder, Render, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    WindowBounds, WindowOptions, anchored, canvas, deferred, div, point, px, rgb, size,
    Corners, PathStyle, StrokeOptions, linear_color_stop, linear_gradient,
};
use lyon_tessellation::{LineCap, LineJoin};
use std::path::PathBuf;
use std::process::Child;
use std::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// Surfaces sit close to black, each a small step above the one under it, so borders and the orange carry the shapes.
const BG: u32 = 0x050506;
const BAR: u32 = 0x09090b; // the title bar, a step above the page
const CARD: u32 = 0x0d0d0f;
const LINE: u32 = 0x202024;
const CHIP: u32 = 0x141417;
const LIT: u32 = 0x202024; // hover light on CHIP controls
const TEXT: u32 = 0xececee;
const MUTED: u32 = 0xb4b4bb; // secondary text: quieter than TEXT, still easy to read on CARD and BG
const ACCENT: u32 = 0xdb5000; // Sunset orange, as on the site
const ACCENT_HI: u32 = 0xf26a12; // its hover
const ON_ACCENT: u32 = 0xffffff;
const RED: u32 = 0xef6b6b; // errors only; orange is the one accent
const SOFT: u32 = 0xd2d2d7; // labels and unselected tabs: a step under TEXT

// One type scale: the page's title, a card's heading, body text, small print.
const T_TITLE: f32 = 30.;
const T_HEAD: f32 = 15.;
const T_BODY: f32 = 13.;
const T_SMALL: f32 = 12.;

// One spacing scale for every page, so gutters, gaps and control heights line up across views.
const GUTTER: f32 = 24.; // window edge to content, left and right
const GAP: f32 = 16.; // between cards, columns and page blocks
const PAD: f32 = 16.; // inside cards and their rows
const CTRL_H: f32 = 36.; // buttons
const BIG_H: f32 = 48.; // the Play row and the banner's other buttons
const T_BIG: f32 = 15.; // their labels
const STRIP_H: f32 = BIG_H + 40.; // the banner's strip under its horizon, the button row centred in it
const BAR_H: f32 = 53.; // the title bar with its 1 px bottom border; tabs and window buttons fill the 52 above it
const TOP_H: f32 = 32.; // the title bar's chips (platform, instance, account), centred in it
const HEAD_H: f32 = 48.; // a card's heading row, so headings in cards side by side share a baseline
const LOG_MAX: f32 = 236.; // the changelog list scrolls past this
const PAGE_MAX: (f32, f32) = (1280., 760.); // Overview's block in a big window
// Corner radii: one radius for the whole window. Cards, menus and dialogs are R_CARD, everything you press is R_CTRL,
// and both are the same. A shape nested close inside another (a tab in the tab strip, a menu row in its menu, the
// hover light inside a bordered button, the face in the account chip) takes the outer radius minus its inset, so both
// corners share one centre and read as the same curve. Round things (status dots, the switch, the progress bar) stay
// round.
const R_CARD: f32 = 10.;
const R_CTRL: f32 = 10.;

/// The radius of a shape inset `by` px inside a corner of radius `outer`.
const fn nested(outer: f32, by: f32) -> f32 {
    if outer - by > 2. { outer - by } else { 2. }
}

/// The account face and the platform mark in their top-bar chips, and how far they sit from the chip's outer edge
/// (the chip's 1 px border included), which their corner radius follows.
const FACE: f32 = 20.;
const FACE_INSET: f32 = (TOP_H - FACE) / 2.;

/// The one instance's name; the instance chip reads as an instance switcher with it.
const INSTANCE_NAME: &str = "Default";

const MAX_CHANGES: usize = 200;
const MAX_CHANGE_CHARS: usize = 400;
// The embedded Inter faces (assets/inter.ttf, inter-bold.ttf) name their family only by their full names. Linux's font
// database knows them as "Inter Medium" and "Inter SemiBold"; DirectWrite groups faces under the family name without
// the weight, so on Windows both are "Inter" and only the weight picks the face. Set both everywhere.
#[cfg(windows)]
const BODY: &str = "Inter";
#[cfg(windows)]
const BOLD: &str = "Inter";
#[cfg(not(windows))]
const BODY: &str = "Inter Medium";
#[cfg(not(windows))]
const BOLD: &str = "Inter SemiBold";
const BODY_WEIGHT: gpui::FontWeight = gpui::FontWeight::MEDIUM;
const BOLD_WEIGHT: gpui::FontWeight = gpui::FontWeight::SEMIBOLD;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Overview,
    Plugins,
    Profiles,
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
enum Menu {
    Account,
    More,
    Window,
    Platform,
    Instance,
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
    /// A menu fading out after it closed, and which close that was (a later close's timer owns it).
    closing: Option<(Menu, u64)>,
    closes: u64,
    /// The menu the backdrop just closed and where that press landed, so the same click on its button doesn't reopen it.
    dismissed: Option<(Menu, Point<Pixels>)>,
    job: Option<Job>,
    note: Option<(bool, String)>,
    sign_in: Option<SignInView>,
    /// Armed by a press on the title bar; the first move then hands the drag to the window manager.
    title_drag: bool,
    playing: bool,
    /// When this launcher started the running game, for the play time it adds when the game exits.
    started: Option<std::time::Instant>,
    sources: Vec<(String, PathBuf)>,
    profiles: Vec<String>,
    mods: Vec<(String, String)>,
    /// The Overview changelog's scroll position.
    log_scroll: gpui::ScrollHandle,
    /// The page's (below the top bar) scroll position, and the tab it was last drawn for.
    page_scroll: gpui::ScrollHandle,
    shown_tab: Tab,
    tx: async_channel::Sender<Msg>,
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
            closing: None,
            closes: 0,
            dismissed: None,
            job: None,
            note: None,
            sign_in: None,
            title_drag: false,
            playing: false,
            started: None,
            sources: vec![],
            profiles: vec![],
            mods: vec![],
            log_scroll: gpui::ScrollHandle::new(),
            page_scroll: gpui::ScrollHandle::new(),
            shown_tab: Tab::Overview,
            tx,
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

    /// Asks horizon-launcher for Horizon instances with Sunset data to import from (Settings lists them).
    fn refresh_sources(&self) {
        if core::cli_path(&self.paths).is_none() {
            return;
        }
        let (p, tx) = (self.paths.clone(), self.send());
        std::thread::spawn(move || {
            let _ = tx.send_blocking(Msg::Sources(core::import_sources(&p)));
        });
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
        self.set_menu(None, cx);
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
                self.started = Some(std::time::Instant::now());
                self.set_settings(|s| s.last_played = Some(core::now_secs()), cx);
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
        self.set_menu(None, cx);
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
                if let Some(t) = self.started.take() {
                    let ran = t.elapsed().as_secs();
                    self.set_settings(|s| s.playtime = s.playtime.saturating_add(ran), cx);
                }
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
        self.set_menu(None, cx);
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
    text(s, size_px, TEXT).font_family(BOLD).font_weight(BOLD_WEIGHT)
}

/// How wide `s` sets in BODY at `size` px.
fn label_width(window: &Window, s: &'static str, size: f32) -> f32 {
    let run = TextRun { len: s.len(), font: gpui::Font { weight: BODY_WEIGHT, ..gpui::font(BODY) }, color: rgb(TEXT).into(), background_color: None, underline: None, strikethrough: None };
    f32::from(window.text_system().shape_line(s.into(), px(size), &[run], None).width)
}

fn card() -> gpui::Div {
    div().bg(rgb(CARD)).border_1().border_color(rgb(LINE)).rounded(px(R_CARD))
}

/// A button in the banner's row: Play's height, label size and spacing, an icon optional before or after the label.
fn big_button(id: impl Into<ElementId>, primary: bool) -> gpui::Stateful<gpui::Div> {
    button_shape(id, primary).h(px(BIG_H)).px(px(20.)).gap(px(8.)).text_size(px(T_BIG))
}

/// A button that does something; there are no disabled ones (what can't work isn't shown).
fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, primary: bool) -> gpui::Stateful<gpui::Div> {
    button_shape(id, primary).child(label.into())
}

/// `button` without its label, for buttons that lay out their own content.
fn button_shape(id: impl Into<ElementId>, primary: bool) -> gpui::Stateful<gpui::Div> {
    let (bg, fg, hover) = if primary { (ACCENT, ON_ACCENT, ACCENT_HI) } else { (CHIP, TEXT, LIT) };
    fx::control(id, hover, if primary { R_CTRL } else { nested(R_CTRL, 1.) })
        .px(px(14.))
        .h(px(CTRL_H))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(R_CTRL))
        .bg(rgb(bg))
        .when(!primary, |d| d.border_1().border_color(rgb(LINE)))
        .text_size(px(13.))
        .text_color(rgb(fg))
        .font_family(BOLD).font_weight(BOLD_WEIGHT)
        .cursor_pointer()
}

enum Icon {
    Play,
    Chevron,
    Check,
    Folder,
    Arrow,
    Minimize,
    Maximize,
    Restore,
    Close,
    User,
}

/// The icons, drawn on one 24-unit grid with 1.75-unit strokes, round ends and joins and rounded corners, so the set
/// reads as one soft family at every size.
fn icon(kind: Icon, s: f32, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b: Bounds<gpui::Pixels>, _, window: &mut Window, _| {
            if matches!(kind, Icon::Minimize | Icon::Maximize | Icon::Restore | Icon::Close) {
                return window_glyph(kind, b, color, window);
            }
            let k = b.size.width / 24.;
            let at = |x: f32, y: f32| point(b.origin.x + k * x, b.origin.y + k * y);
            let stroke = || {
                PathBuilder::fill().with_style(PathStyle::Stroke(
                    StrokeOptions::default().with_line_width(1.75 * f32::from(k)).with_line_cap(LineCap::Round).with_line_join(LineJoin::Round),
                ))
            };
            let line = |p: &mut PathBuilder, pts: &[(f32, f32)]| {
                for (i, (x, y)) in pts.iter().enumerate() {
                    if i == 0 { p.move_to(at(*x, *y)) } else { p.line_to(at(*x, *y)) }
                }
            };
            let p = match kind {
                Icon::Play => {
                    let mut p = PathBuilder::fill();
                    rounded(&mut p, &at, &[(7.5, 5.), (19., 12.), (7.5, 19.)], 2.5);
                    p
                }
                Icon::Chevron => {
                    let mut p = stroke();
                    line(&mut p, &[(7., 10.), (12., 15.), (17., 10.)]);
                    p
                }
                Icon::Check => {
                    let mut p = stroke();
                    line(&mut p, &[(5.5, 12.5), (10., 17.), (18.5, 7.5)]);
                    p
                }
                Icon::Arrow => {
                    let mut p = stroke();
                    line(&mut p, &[(5., 12.), (19., 12.)]);
                    line(&mut p, &[(13., 6.), (19., 12.), (13., 18.)]);
                    p
                }
                Icon::Minimize | Icon::Maximize | Icon::Restore | Icon::Close => return,
                Icon::User => {
                    let mut p = stroke();
                    let head: Vec<(f32, f32)> = (0..=24).map(|i| {
                        let a = std::f32::consts::TAU * i as f32 / 24.;
                        (12. + 3.75 * a.cos(), 8.5 + 3.75 * a.sin())
                    }).collect();
                    line(&mut p, &head);
                    p.move_to(at(5., 19.5));
                    p.curve_to(at(19., 19.5), at(12., 10.5));
                    p
                }
                Icon::Folder => {
                    let mut p = stroke();
                    rounded(&mut p, &at, &[(3.5, 6.), (9.5, 6.), (11.5, 8.5), (20.5, 8.5), (20.5, 18.5), (3.5, 18.5)], 2.);
                    p
                }
            };
            if let Ok(path) = p.build() {
                window.paint_path(path, rgb(color));
            }
        },
    )
    .w(px(s))
    .h(px(s))
    .flex_none()
}

/// The window buttons' glyphs, all in one square: minimise, maximise and restore as 1 px lines on whole device pixels,
/// so they stay sharp at every scale instead of smearing across two pixel rows like a stroked path; close as two
/// diagonals corner to corner.
fn window_glyph(kind: Icon, b: Bounds<Pixels>, color: u32, window: &mut Window) {
    let scale = window.scale_factor();
    let snap = |v: f32| (v * scale).round() / scale;
    let t = scale.round().max(1.) / scale;
    let (bw, bh) = (f32::from(b.size.width), f32::from(b.size.height));
    let side = snap(bw * 10. / 16.);
    let (x0, y0) = (snap(f32::from(b.origin.x) + (bw - side) / 2.), snap(f32::from(b.origin.y) + (bh - side) / 2.));
    let bar = |window: &mut Window, x: f32, y: f32, w: f32, h: f32| window.paint_quad(gpui::fill(Bounds::new(point(px(x), px(y)), size(px(w), px(h))), rgb(color)));
    let square = |window: &mut Window, x: f32, y: f32, s: f32| {
        bar(window, x, y, s, t);
        bar(window, x, y + s - t, s, t);
        bar(window, x, y, t, s);
        bar(window, x + s - t, y, t, s);
    };
    match kind {
        Icon::Minimize => bar(window, x0, snap(y0 + side / 2.), side, t),
        Icon::Maximize => square(window, x0, y0, side),
        Icon::Close => {
            let mut p = PathBuilder::fill().with_style(PathStyle::Stroke(StrokeOptions::default().with_line_width(1.15).with_line_cap(LineCap::Square)));
            let at = |x: f32, y: f32| point(px(x), px(y));
            p.move_to(at(x0 + 0.5, y0 + 0.5));
            p.line_to(at(x0 + side - 0.5, y0 + side - 0.5));
            p.move_to(at(x0 + side - 0.5, y0 + 0.5));
            p.line_to(at(x0 + 0.5, y0 + side - 0.5));
            if let Ok(p) = p.build() {
                window.paint_path(p, rgb(color));
            }
        }
        _ => {
            // The window in front, lower left, and the top and right edges of the one behind it.
            let s = snap(side * 0.8);
            let back = x0 + side - s;
            square(window, x0, y0 + side - s, s);
            bar(window, back + 2. * t, y0, s - 2. * t, t);
            bar(window, x0 + side - t, y0, t, s - 2. * t);
        }
    }
}

/// A closed outline through `pts` (icon grid units) with every corner rounded by up to `r` units.
fn rounded(p: &mut PathBuilder, at: &dyn Fn(f32, f32) -> Point<Pixels>, pts: &[(f32, f32)], r: f32) {
    let toward = |a: (f32, f32), b: (f32, f32)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt().max(1e-3);
        let d = r.min(len / 2.);
        (a.0 + dx / len * d, a.1 + dy / len * d)
    };
    let n = pts.len();
    for i in 0..n {
        let (prev, v, next) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
        let (a, c) = (toward(v, prev), toward(v, next));
        if i == 0 { p.move_to(at(a.0, a.1)) } else { p.line_to(at(a.0, a.1)) }
        p.curve_to(at(c.0, c.1), at(v.0, v.1));
    }
    p.close();
}

/// One line of the changelog: its section as a fixed-width label, then the text, wrapped.
fn change_row(c: &core::Change) -> gpui::Div {
    let kind = match c.kind.trim() {
        "" => "Note".to_string(),
        k => k.chars().take(9).collect(),
    };
    let mut line = c.text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > MAX_CHANGE_CHARS {
        line = line.chars().take(MAX_CHANGE_CHARS).collect::<String>() + "…";
    }
    // The kind is a quiet label in its own column, not a pill that looks pressable.
    let tag = div()
        .flex_none()
        .w(px(72.))
        .pt(px(1.))
        .text_size(px(T_SMALL))
        .line_height(px(18.))
        .font_family(BOLD).font_weight(BOLD_WEIGHT)
        .text_color(rgb(MUTED))
        .child(kind);
    let body = div()
        .flex_1()
        .min_w(px(0.))
        .pt(px(1.))
        .text_size(px(T_BODY))
        .line_height(px(18.))
        .text_color(rgb(TEXT))
        .child(line);
    div().w_full().min_w(px(0.)).flex().items_start().gap(px(10.)).child(tag).child(body)
}

/// The changelog's scroll cues, painted over the list: a fade on an edge with more text past it, and a thin thumb.
/// Painted from the scroll handle each frame, so they follow the list without a re-render.
fn scroll_cues(handle: &gpui::ScrollHandle, window: &mut Window) {
    let b = handle.bounds();
    let (view, max, off) = (f32::from(b.size.height), f32::from(handle.max_offset().y), -f32::from(handle.offset().y));
    if view <= 0. || max <= 0.5 {
        return;
    }
    const FADE: f32 = 24.;
    let (clear, solid) = (Hsla::from(gpui::rgba(CARD << 8)), Hsla::from(rgb(CARD)));
    if off > 0.5 {
        let at = Bounds::new(b.origin, size(b.size.width, px(FADE)));
        window.paint_quad(gpui::fill(at, linear_gradient(180., linear_color_stop(solid, 0.), linear_color_stop(clear, 1.))));
    }
    if off < max - 0.5 {
        let at = Bounds::new(point(b.origin.x, b.origin.y + b.size.height - px(FADE)), size(b.size.width, px(FADE)));
        let r = px(nested(R_CARD, 1.));
        window.paint_quad(
            gpui::fill(at, linear_gradient(180., linear_color_stop(clear, 0.), linear_color_stop(solid, 1.)))
                .corner_radii(Corners { top_left: px(0.), top_right: px(0.), bottom_left: r, bottom_right: r }),
        );
    }
    let track = view - 12.;
    let thumb = (track * view / (view + max)).clamp(24f32.min(track), track);
    let y = 6. + (off / max).clamp(0., 1.) * (track - thumb);
    let at = Bounds::new(point(b.origin.x + b.size.width - px(7.), b.origin.y + px(y)), size(px(3.), px(thumb)));
    window.paint_quad(gpui::fill(at, rgb(0x34343a)).corner_radii(Corners::all(px(1.5))));
}

/// The official Sunset mark (launcher/assets/sunset-launcher.svg, docs/images/logo.png): two concentric half rings,
/// drawn `w` wide and half as tall, its baseline on the box's bottom edge.
fn mark(w: f32, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b: Bounds<gpui::Pixels>, _, window: &mut Window, _| {
            // The SVG's 256-unit grid: the outer ring spans x 33.28..222.72 and stands on y 180.48.
            let k = f32::from(b.size.width) / 189.44;
            let (x0, y0) = (f32::from(b.origin.x) - 33.28 * k, (f32::from(b.origin.y) + f32::from(b.size.height)).round() - 180.48 * k);
            let at = |x: f32, y: f32| point(px(x0 + x * k), px(y0 + y * k));
            // An arc over the top from the left end to the right (or back), centred on (128, cy).
            let arc = |p: &mut PathBuilder, cy: f32, r: f32, back: bool| {
                for i in 0..=64 {
                    let f = if back { 64 - i } else { i } as f32 / 64.;
                    let a = std::f32::consts::PI * (1. + f);
                    p.line_to(at(128. + r * a.cos(), cy + r * a.sin()));
                }
            };
            let mut p = PathBuilder::fill();
            p.move_to(at(33.28, 180.48));
            arc(&mut p, 180.48, 94.72, false);
            arc(&mut p, 180.48, 73.233, true);
            p.close();
            p.move_to(at(74.939, 180.48));
            arc(&mut p, 178.287, 53.061, false);
            p.line_to(at(181.061, 180.48));
            p.line_to(at(159.573, 180.48));
            arc(&mut p, 178.287, 31.573, true);
            p.line_to(at(96.427, 180.48));
            p.close();
            if let Ok(p) = p.build() {
                window.paint_path(p, rgb(color));
            }
        },
    )
    .w(px(w))
    .h(px(w / 2.))
    .flex_none()
}

/// Overview's banner: the Sunset mark setting on a dark horizon, in Sunset orange only, with its glow and reflection
/// (art.rs draws it). It fills its box (a card's shape, border included), so the card can grow with the window.
fn hero_art() -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b: Bounds<gpui::Pixels>, _, window: &mut Window, _| {
            crate::art::paint_hero(window, b, R_CARD, STRIP_H, crate::art::Palette { card: CARD, ember: 0x221003, strip: 0x080706 });
            window.paint_quad(gpui::fill(b, gpui::rgba(0)).corner_radii(Corners::all(px(R_CARD))).border_widths(px(1.)).border_color(rgb(LINE)));
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The banner: `hero_art` behind a title block above the horizon and a button row centred in the strip under it.
fn banner(title: gpui::Div, row: gpui::AnyElement) -> gpui::Div {
    div()
        .relative()
        .flex_1()
        .min_h(px(250.))
        .flex()
        .flex_col()
        .justify_end()
        .child(hero_art())
        .child(title.relative().px(px(28.)).pb(px(24.)))
        .child(div().relative().flex().flex_none().items_center().h(px(STRIP_H)).px(px(28.)).child(row))
}

fn face(pixels: &[[u8; 4]], s: f32) -> impl IntoElement {
    let cell = s / 8.;
    div().flex().flex_col().w(px(s)).h(px(s)).rounded(px(nested(R_CTRL, FACE_INSET))).overflow_hidden().children((0..8).map(|y| {
        div().flex().h(px(cell)).children((0..8).map(move |x| {
            let [r, g, b, _] = pixels[y * 8 + x];
            div().w(px(cell)).h(px(cell)).bg(rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32))
        }))
    }))
}

/// A page's content as one block: at most PAGE_MAX wide and tall, centred in the space under the top bar, so a big or
/// maximised window frames it instead of stretching it into empty areas. In a smaller window it fills the space.
fn page_block(block: gpui::Div) -> gpui::Div {
    div().size_full().flex().flex_col().items_center().justify_center().child(block.w_full().max_w(px(PAGE_MAX.0)).flex_1().max_h(px(PAGE_MAX.1)))
}

/// A card's heading row; cards side by side use it so their headings share one size and baseline.
fn card_head(title: &str) -> gpui::Div {
    div().flex().flex_none().items_center().h(px(HEAD_H)).px(px(PAD)).child(bold(title.to_string(), T_HEAD))
}

fn setting_row(label: impl Into<SharedString>, value: impl IntoElement, last: bool) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(GAP))
        .min_h(px(52.))
        .px(px(PAD))
        .py(px(8.))
        .when(!last, |d| d.border_b_1().border_color(rgb(LINE)))
        // The label takes the rest of the row but never less than LABEL_MIN, wrapping by words; the value keeps its
        // own width while it fits and otherwise shrinks and wraps, right-aligned (a long address or key), instead of
        // squeezing the label down to a letter per line.
        .child(div().flex_1().min_w(px(LABEL_MIN)).child(text(label, 13., TEXT)))
        .child(div().min_w(px(0.)).flex().flex_col().items_end().text_right().child(value))
}

/// The narrowest a settings row's label gets when its value is long.
const LABEL_MIN: f32 = 140.;

fn section(title: &str, rows: Vec<gpui::Div>) -> gpui::Div {
    let n = rows.len();
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(bold(title.to_string(), T_HEAD))
        .child(card().flex().flex_col().children(rows.into_iter().enumerate().map(move |(i, r)| r.when(i + 1 == n, |d| d.border_b_0()))))
}

/// An error line from `core` as a sentence: a capital first letter, the address in brackets dropped (a button opens
/// it), a full stop.
fn sentence(e: &str) -> String {
    let mut s = match e.find(" (") {
        Some(i) if e.ends_with(')') && e[i..].contains('/') => e[..i].to_string(),
        _ => e.trim().to_string(),
    };
    if let Some(c) = s.chars().next() {
        s.replace_range(..c.len_utf8(), &c.to_uppercase().to_string());
    }
    if !s.ends_with(['.', '!', '?', '…']) {
        s.push('.');
    }
    s
}

fn window_label(w: &str) -> String {
    if w == "fullscreen" { "Fullscreen".into() } else { w.replace('x', " × ") }
}

/// A page's title row, the same height on every page whether or not it has a button on the right.
fn page_head(title: &str, action: Option<gpui::AnyElement>) -> gpui::Div {
    div().flex().flex_none().items_center().justify_between().h(px(CTRL_H)).child(bold(title.to_string(), T_TITLE)).children(action)
}

/// The top bar's chips (platform, instance, account) share this shape.
fn top_chip(id: impl Into<ElementId>) -> gpui::Stateful<gpui::Div> {
    fx::control(id, LIT, nested(R_CTRL, 1.))
        .flex()
        .items_center()
        .gap(px(8.))
        .h(px(TOP_H))
        .px(px(12.))
        .rounded(px(R_CTRL))
        .bg(rgb(CHIP))
        .border_1()
        .border_color(rgb(LINE))
        .cursor_pointer()
}

/// A menu row label with a checkmark on the right when `on`.
fn checked(label: impl Into<SharedString>, on: bool) -> gpui::Div {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_between()
        .gap(px(16.))
        .child(label.into())
        .child(if on { icon(Icon::Check, 14., ACCENT).into_any_element() } else { div().w(px(14.)).into_any_element() })
}

/// Minimise, maximise/restore and close, drawn by us when the window has no system title bar (always on Windows,
/// where the system still hit-tests them as caption buttons; on Linux when the desktop accepted client decorations).
fn window_controls(window: &Window) -> Option<gpui::Div> {
    if !cfg!(windows) && !matches!(window.window_decorations(), Decorations::Client { .. }) {
        return None;
    }
    let ctl = |id: &'static str, kind: Icon, area: WindowControlArea, hover: u32| {
        // Square, full bar height and flush with the window's top-right corner, like any desktop title bar.
        fx::control(id, hover, 0.)
            .w(px(46.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .window_control_area(area)
            .child(icon(kind, 16., TEXT))
    };
    // Windows answers these areas itself (as caption buttons), so the clicks are only ours elsewhere. There the press
    // must reach the platform layer: it only remembers which caption button went down (and acts on its release) when
    // no element stops the press, so these sit outside the top bar's press-taking groups. `occlude` keeps the drag
    // area behind them out of the hit test (gpui answers Windows with the first control area under the pointer) and
    // keeps the top bar's own press handler, which arms a window drag, from firing under them.
    let own = !cfg!(windows);
    let max = if window.is_maximized() { Icon::Restore } else { Icon::Maximize };
    Some(
        div()
            .flex()
            .items_center()
            .flex_none()
            .h_full()
            .ml(px(12.))
            .mr(px(-GUTTER))
            .occlude()
            .child(ctl("win-min", Icon::Minimize, WindowControlArea::Min, LIT).when(own, |d| d.on_click(|_, window, _| window.minimize_window())))
            .child(ctl("win-max", max, WindowControlArea::Max, LIT).when(own, |d| d.on_click(|_, window, _| window.zoom_window())))
            .child(ctl("win-close", Icon::Close, WindowControlArea::Close, 0xc42b1c).when(own, |d| d.on_click(|_, _, cx| cx.quit()))),
    )
}

/// Thin strips along the edges and corners that start a resize, for client-decorated windows on Linux (Windows
/// hit-tests its own borders). Edges tiled against the screen or a maximised window get none.
fn resize_edges(window: &Window) -> Option<gpui::Div> {
    if cfg!(windows) || window.is_maximized() || window.is_fullscreen() {
        return None;
    }
    let Decorations::Client { tiling } = window.window_decorations() else { return None };
    const E: f32 = 6.;
    let strip = |edge: ResizeEdge, cursor: CursorStyle| {
        div().absolute().cursor(cursor).occlude().on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            window.start_window_resize(edge);
        })
    };
    let (t, b, l, r) = (!tiling.top, !tiling.bottom, !tiling.left, !tiling.right);
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .when(t, |d| d.child(strip(ResizeEdge::Top, CursorStyle::ResizeUpDown).top_0().left(px(E)).right(px(E)).h(px(E))))
            .when(b, |d| d.child(strip(ResizeEdge::Bottom, CursorStyle::ResizeUpDown).bottom_0().left(px(E)).right(px(E)).h(px(E))))
            .when(l, |d| d.child(strip(ResizeEdge::Left, CursorStyle::ResizeLeftRight).left_0().top(px(E)).bottom(px(E)).w(px(E))))
            .when(r, |d| d.child(strip(ResizeEdge::Right, CursorStyle::ResizeLeftRight).right_0().top(px(E)).bottom(px(E)).w(px(E))))
            .when(t && l, |d| d.child(strip(ResizeEdge::TopLeft, CursorStyle::ResizeUpLeftDownRight).top_0().left_0().size(px(E))))
            .when(t && r, |d| d.child(strip(ResizeEdge::TopRight, CursorStyle::ResizeUpRightDownLeft).top_0().right_0().size(px(E))))
            .when(b && l, |d| d.child(strip(ResizeEdge::BottomLeft, CursorStyle::ResizeUpRightDownLeft).bottom_0().left_0().size(px(E))))
            .when(b && r, |d| d.child(strip(ResizeEdge::BottomRight, CursorStyle::ResizeUpLeftDownRight).bottom_0().right_0().size(px(E)))),
    )
}

// ---------------------------------------------------------------- views

impl Launcher {
    /// The top bar doubles as the window's title bar: drag it to move, double-click to maximise. Its buttons sit in
    /// groups that take the press for themselves (and block the drag area under them for Windows' hit test).
    fn top_bar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let has = self.installed.is_some();
        let tabs = [(Tab::Overview, "Overview"), (Tab::Plugins, "Plugins"), (Tab::Profiles, "Profiles"), (Tab::Settings, "Settings")];
        let group = || div().flex().items_center().gap(px(8.)).occlude().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        // The mark and the two switchers, then the tabs: a desktop title bar, read left to right, the window's
        // own buttons at the far right and the free space between as the drag area.
        let left = group()
            .child(div().flex_none().pr(px(8.)).child(mark(32., ACCENT)))
            .child(self.dropdown(Menu::Platform, "Horizon", cx))
            .child(self.dropdown(Menu::Instance, if has { INSTANCE_NAME } else { "No instance" }, cx));
        // All four tabs always, so they never move; with no instance Plugins and Profiles say what to do. Each fills
        // the bar's height like the window buttons; the selected one is lit and underlined in orange on the bar's
        // bottom edge. Widths come from the labels, so the underline that slides between them knows where each one is
        // in the same frame.
        let spans: Vec<(f32, f32)> = {
            let mut x = 0.;
            tabs.iter()
                .map(|(_, label)| {
                    let w = (label_width(window, label, 13.) + 28.).ceil();
                    x += w;
                    (x - w, w)
                })
                .collect()
        };
        let widths: Vec<f32> = spans.iter().map(|s| s.1).collect();
        let underline = tabs.iter().position(|(t, _)| *t == self.tab).map(|at| {
            div().absolute().bottom_0().h(px(2.)).rounded_t(px(1.)).bg(rgb(ACCENT)).with_spring(
                "tab-line",
                SpringAnimation::new(fx::SLIDE).to(at as f32).with_epsilon(0.002),
                move |d, p| {
                    let (left, w) = fx::pill_span(&spans, p);
                    d.left(px(left + 10.)).w(px(w - 20.))
                },
            )
        });
        let tab_strip = group().gap_0().h_full().child(div().w(px(1.)).h(px(20.)).mx(px(12.)).bg(rgb(LINE))).child(
            div().relative().flex().h_full().children(underline).children(tabs.into_iter().zip(widths).map(|((t, label), w)| {
                let on = self.tab == t;
                let id = ElementId::from(label);
                let lit = on || fx::hovered(&id);
                fx::pressable(id, None, 0.)
                    .w(px(w))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(13.))
                    .font_family(BOLD).font_weight(BOLD_WEIGHT)
                    .child(div().child(label).with_spring(
                        "tab-text",
                        SpringAnimation::new(fx::HOVER).to(if lit { 1f32 } else { 0. }).with_epsilon(0.004),
                        move |d, t| d.text_color(fx::mix(MUTED, TEXT, t)),
                    ))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.tab = t;
                        this.set_menu(None, cx);
                        if t != Tab::Overview {
                            this.reload_local();
                        }
                        if t == Tab::Settings {
                            this.refresh_sources();
                        }
                        cx.notify();
                    }))
            })),
        );
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(BAR_H))
            // Edge to edge across the window, its contents still on the page's gutter.
            .mx(px(-GUTTER))
            .px(px(GUTTER))
            .bg(rgb(BAR))
            .border_b_1()
            .border_color(rgb(LINE))
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, window, _| {
                this.title_drag = ev.click_count < 2;
                if ev.click_count == 2 {
                    window.zoom_window();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, _| this.title_drag = false))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, window, _| {
                if std::mem::take(&mut this.title_drag) && ev.pressed_button == Some(MouseButton::Left) {
                    window.start_window_move();
                }
            }))
            .child(left)
            .child(tab_strip)
            .child(div().flex_1().min_w(px(12.)).h_full())
            .child(group().child(self.account_chip(cx)))
            .children(window_controls(window))
    }

    /// A top-bar chip that toggles `menu`; its list drops down under it (the backdrop is in `menu_popup`).
    fn dropdown(&self, menu: Menu, label: &'static str, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let id = if menu == Menu::Platform { "platform" } else { "instance" };
        let chip = top_chip(id)
            .child(text(label, 13., TEXT))
            .child(icon(Icon::Chevron, 14., MUTED))
            .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| this.toggle_menu(menu, ev, cx)));
        self.with_menu(menu, chip, false, cx)
    }

    /// `trigger` with `menu`'s list dropping down under it while open, lined up with its left or right edge. The
    /// backdrop that closes it is in `menu_popup`.
    fn with_menu(&self, menu: Menu, trigger: impl IntoElement, right: bool, cx: &mut Context<Self>) -> gpui::Div {
        let open = self.menu == Some(menu);
        let closing = !open && self.closing.is_some_and(|(m, _)| m == menu);
        let list = (open || closing).then(|| {
            let pop = self.menu_items(menu, card().p(px(4.)).min_w(px(200.)).flex().flex_col().shadow_lg().occlude(), cx);
            // In: fade up from 0 while dropping the last 4 px into place, fast then settling. Out: the same backwards,
            // a touch quicker and accelerating away.
            let (id, anim) = if open {
                ("menu-in", Animation::new(fx::MENU_IN).with_easing(ease_out_quint()))
            } else {
                ("menu-out", Animation::new(fx::MENU_OUT).with_easing(quadratic))
            };
            let pop = pop.with_animation((id, menu as usize), anim, move |d, t| {
                let v = if open { t } else { 1. - t };
                d.opacity(v).mt(px(2. + 4. * v))
            });
            let at = if right { anchored().anchor(Anchor::TopRight) } else { anchored() };
            div().absolute().top_full().when(right, |d| d.right_0()).when(!right, |d| d.left_0()).child(deferred(at.child(pop)).with_priority(1))
        });
        div().relative().flex_none().child(trigger).children(list)
    }

    fn menu_items(&self, menu: Menu, pop: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        match menu {
            // Reserved for Sunrise/Java later; Horizon is the only platform today.
            Menu::Platform => pop.child(self.menu_row("p-horizon", checked("Horizon", true), cx, |_, _| {})),
            Menu::Instance => {
                let pop = match self.installed {
                    Some(_) => pop.child(self.menu_row("i-sunset", checked(INSTANCE_NAME, true), cx, |this, _| this.tab = Tab::Overview)),
                    None => pop,
                };
                pop.child(self.menu_item("i-new", "New instance", cx, |this, cx| this.new_instance(cx)))
            }
            Menu::Account => {
                let mut pop = pop;
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
                pop.child(self.menu_item("m-logs", "Open logs", cx, move |this, cx| this.open(logs.clone(), cx)))
                    .child(self.menu_item("m-check", "Check for updates", cx, |this, _| this.check(false)))
                    .child(self.menu_item("m-repair", "Repair installation", cx, |this, cx| this.start_install(true, cx)))
            }
            Menu::Window => {
                let mut pop = pop;
                for w in core::WINDOW_SIZES {
                    pop = pop.child(self.menu_item(SharedString::from(w), window_label(w), cx, move |this, cx| this.set_settings(|s| s.window = w.into(), cx)));
                }
                pop
            }
        }
    }

    /// The menu's New instance: the same install the empty Overview offers. Only one instance exists for now, so
    /// with it installed this says so instead of reinstalling over it.
    fn new_instance(&mut self, cx: &mut Context<Self>) {
        self.tab = Tab::Overview;
        if self.installed.is_some() {
            self.note = Some((false, "Sunset has one instance for now, and it is already installed. Repair it from Overview.".into()));
        } else {
            self.start_install(false, cx);
        }
    }

    fn account_chip(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(a) = self.accounts.iter().find(|a| a.selected).or(self.accounts.first()) else {
            // The same chip as when signed in, with a person in place of the face.
            return top_chip("sign-in")
                .pl(px(8.))
                .child(icon(Icon::User, 16., TEXT))
                .child(text("Sign in", 13., TEXT))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_sign_in(cx)))
                .into_any_element();
        };
        let head = self.faces.iter().find(|(u, _)| u == &a.uuid).map(|(_, f)| face(f, FACE).into_any_element()).unwrap_or_else(|| div().w(px(FACE)).h(px(FACE)).rounded(px(nested(R_CTRL, FACE_INSET))).bg(rgb(0x2a2a2f)).into_any_element());
        let chip = top_chip("account")
            .pl(px(FACE_INSET - 1.))
            .pr(px(10.))
            .child(head)
            .child(text(a.name.clone(), 13., TEXT))
            .child(icon(Icon::Chevron, 14., MUTED))
            .on_click(cx.listener(|this, ev: &ClickEvent, _, cx| this.toggle_menu(Menu::Account, ev, cx)));
        self.with_menu(Menu::Account, chip, true, cx).into_any_element()
    }

    fn menu_item(&self, id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &mut Context<Self>, f: impl Fn(&mut Self, &mut Context<Self>) + 'static) -> gpui::Stateful<gpui::Div> {
        self.menu_row(id, div().child(label.into()), cx, f)
    }

    fn menu_row(&self, id: impl Into<ElementId>, label: gpui::Div, cx: &mut Context<Self>, f: impl Fn(&mut Self, &mut Context<Self>) + 'static) -> gpui::Stateful<gpui::Div> {
        // Inside the menu's 1 px border and 4 px padding.
        fx::control(id, LIT, nested(R_CARD, 5.))
            .px(px(12.))
            .h(px(32.))
            .flex()
            .items_center()
            .rounded(px(nested(R_CARD, 5.)))
            .cursor_pointer()
            .text_size(px(13.))
            .text_color(rgb(TEXT))
            .child(label)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                // A menu fading out still draws its rows; they no longer act.
                if this.menu.is_none() {
                    return;
                }
                this.set_menu(None, cx);
                f(this, cx);
                cx.notify();
            }))
    }

    /// Opens `menu`, or closes it when it is already open. The backdrop closes menus on mouse down, before the
    /// button's click arrives, so a click on the open menu's own button is recognised by its press position.
    fn toggle_menu(&mut self, menu: Menu, ev: &ClickEvent, cx: &mut Context<Self>) {
        let dismissed = self.dismissed.take();
        let just_closed = matches!((dismissed, ev), (Some((m, at)), ClickEvent::Mouse(c)) if m == menu && at == c.down.position);
        self.set_menu(if self.menu == Some(menu) || just_closed { None } else { Some(menu) }, cx);
        cx.notify();
    }

    /// Every menu opens and closes through here, so a closing menu stays drawn while it fades out.
    fn set_menu(&mut self, next: Option<Menu>, cx: &mut Context<Self>) {
        if let Some(old) = self.menu.filter(|m| Some(*m) != next) {
            self.closes += 1;
            let close = self.closes;
            self.closing = Some((old, close));
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(fx::MENU_OUT + Duration::from_millis(30)).await;
                let _ = this.update(cx, |app, cx| {
                    if app.closing.is_some_and(|(_, c)| c == close) {
                        app.closing = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        if next.is_some() && self.closing.map(|(m, _)| m) == next {
            self.closing = None;
        }
        self.menu = next;
    }

    /// The backdrop under an open menu (the menus themselves hang off their buttons, see `with_menu`).
    fn menu_popup(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        self.menu?;
        Some(div().id("menu-backdrop").absolute().size_full().top_0().left_0().on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, _, cx| {
            this.dismissed = this.menu.map(|m| (m, ev.position));
            this.set_menu(None, cx);
            cx.notify();
        })))
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
                .child(div().h(px(6.)).w_full().rounded_full().bg(rgb(0x1c1c20)).child(div().h_full().rounded_full().bg(rgb(ACCENT)).w(gpui::relative(if j.frac.is_some() { frac } else { 1. })).when(j.frac.is_none(), |d| d.opacity(0.35)))),
        )
    }

    fn overview(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(m) = self.installed.as_ref() else { return self.no_instance(cx).into_any_element() };
        let update = self.update_available().map(|u| u.version.clone());
        let play_area = match self.progress(cx) {
            Some(p) => div().w(px(460.)).max_w_full().child(p).into_any_element(),
            None => div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    if self.playing { div().id("play") } else { fx::control("play", ACCENT_HI, R_CTRL) }
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(8.))
                        .w(px(220.))
                        .h(px(BIG_H))
                        .rounded(px(R_CTRL))
                        .bg(rgb(ACCENT))
                        .when(self.playing, |d| d.opacity(0.6))
                        .when(!self.playing, |d| d.cursor_pointer())
                        .child(icon(Icon::Play, 18., ON_ACCENT))
                        .child(div().text_size(px(T_BIG)).font_family(BOLD).font_weight(BOLD_WEIGHT).text_color(rgb(ON_ACCENT)).child(if self.playing { "Playing" } else { "Play" }))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.play(cx))),
                )
                .when_some(update, |d, v| d.child(big_button("update", false).child(format!("Update to {v}")).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_install(false, cx)))))
                .child(big_button("open-folder", false).child(icon(Icon::Folder, 18., TEXT)).child("Game folder").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    let p = this.paths.instance();
                    this.open(p, cx)
                })))
                .child(self.with_menu(
                    Menu::More,
                    big_button("more", false).pr(px(16.)).child("Logs and repair").child(icon(Icon::Chevron, 16., TEXT)).on_click(cx.listener(|this, ev: &ClickEvent, _, cx| this.toggle_menu(Menu::More, ev, cx))),
                    false,
                    cx,
                ))
                .child(div().flex_1())
                .children(self.play_stats())
                .w_full()
                .into_any_element(),
        };
        // The banner says what this is once: Sunset, its version and the game it runs. Play is the first thing under it.
        let hero = banner(
            div().flex().flex_col().gap(px(4.)).child(bold("Sunset", T_TITLE)).child(text(format!("Version {} for Minecraft {}", m.version, m.horizon.game), 14., TEXT)),
            play_area,
        );
        let plugins = self.mods.iter().filter(|(id, _)| id != "sunset").count();
        let facts = vec![
            ("Horizon build", m.horizon_build()),
            ("Profiles", if self.profiles.is_empty() { "None saved".to_string() } else { format!("{} saved", self.profiles.len()) }),
            ("Plugins", match plugins {
                0 => "None yet".to_string(),
                1 => "1 built into Sunset".to_string(),
                n => format!("{n} built into Sunset"),
            }),
        ];
        let n = facts.len();
        let right = card()
            .w(px(320.))
            .flex_none()
            .flex()
            .flex_col()
            .pb(px(8.))
            .child(card_head("Installation"))
            .children(facts.into_iter().enumerate().map(|(i, (k, v))| {
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(38.))
                    .mx(px(PAD))
                    .when(i + 1 < n, |d| d.border_b_1().border_color(rgb(LINE)))
                    .child(text(k, T_BODY, SOFT))
                    .child(text(v, T_BODY, TEXT).font_family(BOLD).font_weight(BOLD_WEIGHT))
            }));
        let block = div().flex().flex_col().gap(px(GAP)).when_some(self.import_prompt(cx), |d, p| d.child(p)).child(hero).child(div().flex().flex_none().gap(px(GAP)).child(self.changelog(m)).child(right));
        page_block(block).into_any_element()
    }

    /// When this launcher last started the game and how long it has run, for the right of the Play row. Only what
    /// the launcher recorded itself; nothing until it has.
    fn play_stats(&self) -> Vec<gpui::Div> {
        let stat = |label: &'static str, value: String| div().flex().flex_col().items_end().gap(px(2.)).child(text(label, T_SMALL, MUTED)).child(bold(value, 15.));
        let mut v = vec![];
        if let Some(t) = self.settings.last_played {
            v.push(stat("Last played", core::ago(t, core::now_secs())));
        }
        if self.settings.playtime > 0 {
            v.push(stat("Play time", core::play_time(self.settings.playtime)).ml(px(28.)));
        }
        v
    }

    /// The installed version's changelog, from its signed manifest: one card, its lines scrolling inside it past
    /// LOG_MAX, with edge fades and a thin thumb when there is more (`scroll_cues`).
    fn changelog(&self, m: &Manifest) -> gpui::Div {
        let mut changes: Vec<&core::Change> = m.changelog.iter().filter(|c| !c.text.trim().is_empty()).take(MAX_CHANGES).collect();
        // The release's highlights (bold in CHANGELOG.md) lead; every line is then set the same.
        changes.sort_by_key(|c| !c.important);
        // min_w(0): long lines wrap inside the card instead of widening it past the window.
        let panel = card().relative().flex_1().min_w(px(0.)).flex().flex_col().overflow_hidden().child(card_head("What's new in this version"));
        if changes.is_empty() {
            return panel.child(div().px(px(PAD)).pb(px(PAD)).child(text("This version came without a list of changes.", T_BODY, TEXT)));
        }
        let handle = self.log_scroll.clone();
        panel
            .child(
                div()
                    .id("changelog")
                    .w_full()
                    .max_h(px(LOG_MAX))
                    .overflow_y_scroll()
                    .track_scroll(&self.log_scroll)
                    .child(div().flex_none().w_full().flex().flex_col().gap(px(12.)).px(px(PAD)).pb(px(PAD)).children(changes.into_iter().map(change_row))),
            )
            .child(fx::smooth_scroll("changelog", &self.log_scroll))
            .child(canvas(|_, _, _| (), move |_, _, window: &mut Window, _| scroll_cues(&handle, window)).absolute().top_0().left_0().size_full())
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
                .gap(px(12.))
                .p(px(PAD))
                .child(div().flex_1().child(text(format!("Bring your Sunset configs and profiles over from the Horizon instance \"{name}\"? Files already here are kept."), 13., TEXT)))
                .child(button("import-0", "Import", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.import(0, cx))))
                .child(button("import-no", "Not now", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.set_settings(|s| s.import_decided = true, cx)))),
        )
    }

    fn no_instance(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let failed = self.latest.is_none() && self.latest_err.is_some();
        let action = match self.progress(cx) {
            Some(p) => div().w(px(460.)).max_w_full().child(p).into_any_element(),
            None if self.latest.is_some() => big_button("new-instance", true)
                .px(px(28.))
                .child("Install Sunset")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_install(false, cx)))
                .into_any_element(),
            None if failed => div()
                .flex()
                .gap(px(8.))
                .child(big_button("retry", true).child("Try again").on_click(cx.listener(|this, _: &ClickEvent, _, _| this.check(false))))
                .child(big_button("releases", false).child("Open the releases page").on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.open_url(&format!("https://github.com/{}/releases", core::REPO)))))
                .into_any_element(),
            None => div().into_any_element(),
        };
        let about = match (&self.latest, &self.latest_err) {
            (Some(r), _) => text(format!("Version {} for Minecraft {} is ready to install.", r.manifest.version, r.manifest.horizon.game), 14., TEXT),
            // A failed check reads as a status: a red dot and a sentence, the address behind the button instead.
            (None, Some(e)) => div().flex().items_center().gap(px(8.)).child(div().flex_none().w(px(8.)).h(px(8.)).rounded_full().bg(rgb(RED))).child(text(sentence(e), 14., TEXT)),
            (None, None) => text("Looking for the latest Sunset release…", 14., TEXT),
        };
        // The same banner as Overview with the install in place of Play, and the release's changelog under it.
        let hero = banner(div().flex().flex_col().gap(px(6.)).child(bold("Install Sunset", T_TITLE)).child(about), action);
        let block = div().flex().flex_col().gap(px(GAP)).child(hero).when_some(self.latest.as_ref(), |d, r| d.child(div().flex().flex_none().child(self.changelog(&r.manifest))));
        page_block(block)
    }

    /// Settings as one centred column: the installed instance, updates and files, importing from Horizon, and About.
    /// One column of sections reads top to bottom, instead of two unbalanced stacks of cards side by side.
    fn settings_view(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut sections: Vec<gpui::Div> = Vec::new();
        let root = self.paths.root.clone();

        if let Some(m) = self.installed.as_ref() {
            let toggle = {
                let on = self.settings.auto_update;
                let slide = SpringAnimation::new(fx::SLIDE).to(if on { 1f32 } else { 0. }).with_epsilon(0.002);
                div()
                    .id("auto-update")
                    .relative()
                    .w(px(38.))
                    .h(px(22.))
                    .rounded_full()
                    .bg(rgb(0x3a3a40))
                    .cursor_pointer()
                    .child(div().size_full().rounded_full().bg(rgb(ACCENT)).with_spring("track", slide.clone(), |d, t| d.opacity(t)))
                    .child(
                        div()
                            .absolute()
                            .top(px(3.))
                            .size(px(16.))
                            .rounded_full()
                            .bg(rgb(MUTED))
                            .child(div().size_full().rounded_full().bg(rgb(ON_ACCENT)).with_spring("knob-lit", slide.clone(), |d, t| d.opacity(t)))
                            .with_spring("knob", slide, |d, t| d.left(px(3. + 16. * t))),
                    )
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.set_settings(|s| s.auto_update = !s.auto_update, cx)))
            };
            let version = match self.update_available() {
                Some(u) => button("s-update", format!("{} / Update to {}", m.version, u.version), false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_install(false, cx))).into_any_element(),
                None => text(m.version.clone(), 13., TEXT).into_any_element(),
            };
            let (inst, data, logs) = (self.paths.instance(), root.clone(), self.paths.logs());

            sections.push(section("Instance", vec![
                setting_row("Name", text(INSTANCE_NAME, 13., TEXT), false),
                setting_row("Sunset version", version, false),
                setting_row("Horizon", text(format!("{} (commit {})", m.horizon_build(), m.short_pin()), 13., TEXT), false),
                setting_row(
                    "Window size",
                    self.with_menu(Menu::Window, button("s-window", window_label(&self.settings.window), false).on_click(cx.listener(|this, ev: &ClickEvent, _, cx| this.toggle_menu(Menu::Window, ev, cx))), true, cx),
                    true,
                ),
            ]));
            sections.push(section("Updates", vec![
                setting_row("Install new Sunset releases automatically", toggle, false),
                setting_row("Check for updates", button("s-check", "Check", false).on_click(cx.listener(|this, _: &ClickEvent, _, _| this.check(false))), true),
            ]));
            sections.push(section("Files", vec![
                setting_row("Instance directory", button("s-open", "Open folder", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(inst.clone(), cx))), false),
                setting_row("Launcher data folder", button("l-data", "Open folder", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(data.clone(), cx))), false),
                setting_row("Logs", button("s-logs", "Open logs", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(logs.clone(), cx))), false),
                setting_row("Repair installation (check and download again)", button("s-repair", "Repair", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_install(true, cx))), true),
            ]));
        } else {
            sections.push(section("Files", vec![
                setting_row("Launcher data folder", button("l-data", "Open folder", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(root.clone(), cx))), true),
            ]));
        }

        sections.push(self.import_section(cx));
        sections.push(self.about_section());

        // Full width, like the Plugins and Profiles pages: page title, then the sections stacked down the page.
        div()
            .flex()
            .flex_col()
            .gap(px(GAP))
            .child(page_head("Settings", None))
            .when_some(self.progress(cx), |d, p| d.child(card().p(px(PAD)).child(p)))
            .children(sections)
            .into_any_element()
    }

    /// Importing Sunset's config and profiles from another Horizon instance: one row per source found.
    fn import_section(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut rows: Vec<gpui::Div> = self
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
        if rows.is_empty() {
            rows.push(setting_row("No Horizon instance with Sunset configs found", div(), true));
        }
        section("Import Sunset configs and profiles from Horizon", rows)
    }

    /// The launcher's own facts: version, where it installs from, the stock CLI it drives, its signing key, and what it is.
    fn about_section(&self) -> gpui::Div {
        let source = std::env::var("SUNSET_MANIFEST").ok().filter(|s| !s.is_empty()).or(self.settings.manifest_url.clone()).unwrap_or_else(|| format!("GitHub Releases ({})", core::REPO));
        let cli = core::cli_path(&self.paths).map(|p| p.parent().and_then(|d| d.file_name()).map(|v| v.to_string_lossy().into_owned()).unwrap_or_default());
        let key = include_str!("../sunset.pub").lines().next().unwrap_or("").rsplit(' ').next().unwrap_or("").to_string();
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(bold("About", T_HEAD))
            .child(
                card()
                    .flex()
                    .flex_col()
                    .child(setting_row("Sunset launcher", text(core::VERSION, 13., TEXT), false))
                    .child(setting_row("Platform", text(core::PLATFORM, 13., TEXT), false))
                    .child(setting_row("Licence", text("PolyForm Strict 1.0.0", 13., TEXT), false))
                    .child(setting_row("Release source", text(source, T_SMALL, MUTED), false))
                    .child(setting_row("horizon-launcher (sign-in, install, play)", text(cli.unwrap_or_else(|| "comes with the first install".into()), 13., TEXT), false))
                    .child(setting_row("Signing key", text(key, T_SMALL, MUTED), false))
                    .child(div().p(px(PAD)).flex().flex_col().gap(px(8.)).child(text("Installs the signed prebuilt Sunset client for the Horizon Nightly it was built and tested on, and plays it through the stock horizon-launcher, which keeps your Microsoft sign-in.", T_SMALL, MUTED)).child(text("Horizon is LGPL-3.0-or-later; the exact source each prebuilt is built from is published with it. Not affiliated with Mojang or Microsoft.", T_SMALL, MUTED))),
            )
    }

    fn list_view(&self, title: &str, empty: String, items: Vec<(String, String)>, folder: Option<PathBuf>, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let n = items.len();
        div()
            .flex()
            .flex_col()
            .gap(px(GAP))
            .child(page_head(title, folder.map(|f| button("list-folder", "Open folder", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(f.clone(), cx))).into_any_element())))
            .child(if items.is_empty() {
                card().p(px(PAD)).child(text(empty, 13., MUTED))
            } else {
                card().flex().flex_col().children(items.into_iter().enumerate().map(move |(i, (a, b))| setting_row(a, text(b, 13., MUTED), i + 1 == n)))
            })
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
                        .child(div().px(px(16.)).py(px(8.)).rounded(px(R_CTRL)).bg(rgb(CHIP)).border_1().border_color(rgb(LINE)).child(text(code.clone(), 26., TEXT).font_family(BOLD).font_weight(BOLD_WEIGHT)))
                        .child(button("si-copy", if s.copied { "Copied" } else { "Copy code" }, true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(c.clone()));
                            if let Some(s) = this.sign_in.as_mut() {
                                s.copied = true;
                            }
                            cx.notify();
                        }))),
                )
                .child(text("Waiting for you to finish in the browser…", T_SMALL, MUTED));
        } else {
            body = body.child(text("Asking Microsoft for a sign-in code…", 13., MUTED));
        }
        body = body.child(div().flex().justify_end().child(button("si-cancel", "Cancel", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_sign_in(cx)))));
        Some(div().absolute().top_0().left_0().size_full().flex().items_center().justify_center().bg(Hsla::from(gpui::rgba(0x000000b0))).child(body))
    }

}

impl Render for Launcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        fx::begin_frame();
        // Another page (however the tab changed) starts at its top.
        if self.shown_tab != self.tab {
            self.shown_tab = self.tab;
            fx::stop_scroll("page");
            self.page_scroll.set_offset(point(px(0.), px(0.)));
        }
        let page = match self.tab {
            Tab::Overview => self.overview(cx),
            Tab::Settings => self.settings_view(cx),
            Tab::Plugins => {
                let items = self.mods.iter().map(|(id, v)| (if id == "sunset" { "Sunset".to_string() } else { id.clone() }, v.clone())).collect();
                let empty = if self.installed.is_some() { "Nothing built into this version yet." } else { "Install Sunset from Overview to see the plugins built into it." };
                self.list_view("Plugins", empty.into(), items, None, cx).into_any_element()
            }
            Tab::Profiles => {
                let items = self.profiles.iter().map(|p| (p.clone(), String::new())).collect();
                let empty = "No saved profiles yet. Save one in game from Sunset's menu (Right Shift).".to_string();
                let folder = self.installed.is_some().then(|| self.paths.sunset_data().join("profiles"));
                self.list_view("Profiles", empty, items, folder, cx).into_any_element()
            }
        };
        let note = self.note.clone().map(|(err, msg)| {
            card()
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(PAD))
                .py(px(10.))
                .child(div().w(px(7.)).h(px(7.)).rounded_full().bg(rgb(if err { RED } else { ACCENT })))
                .child(div().flex_1().child(text(msg, 13., if err { RED } else { TEXT })))
                .child(div().id("note-close").cursor_pointer().child(icon(Icon::Close, 16., MUTED)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.note = None;
                    cx.notify();
                })))
        });
        div()
            .size_full()
            .relative()
            .bg(rgb(BG))
            .font_family(BODY).font_weight(BODY_WEIGHT)
            .text_color(rgb(TEXT))
            .flex()
            .flex_col()
            .px(px(GUTTER))
            .pb(px(GUTTER))
            .child(self.top_bar(window, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .mt(px(GAP))
                    .flex()
                    .flex_col()
                    .child(div().id("page").flex_1().min_h(px(0.)).overflow_y_scroll().track_scroll(&self.page_scroll).child(page))
                    .child(fx::smooth_scroll("page", &self.page_scroll)),
            )
            .when_some(note, |d, n| d.child(n.mt(px(GAP))))
            .when_some(self.menu_popup(cx), |d, m| d.child(m))
            .when_some(self.sign_in_modal(cx), |d, m| d.child(m))
            .when_some(resize_edges(window), |d, e| d.child(e))
    }
}

/// Linux: a desktop entry and icon for the window's app id (`sunset-launcher`) under $XDG_DATA_HOME, which is where
/// Wayland compositors and X11 docks look up a window's icon. Written when missing or different (the launcher moved);
/// a failure only costs the icon. Windows takes the icon from the executable (build.rs).
#[cfg(target_os = "linux")]
fn desktop_entry() {
    let Some(data) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    else {
        return;
    };
    let Ok(exe) = std::env::current_exe() else { return };
    // Exec's quoting: a backslash before " ` $ and \, then the string escape doubles every backslash; % is %%.
    let exe = exe.to_string_lossy().replace('\\', "\\\\\\\\").replace('"', "\\\\\"").replace('`', "\\\\`").replace('$', "\\\\$").replace('%', "%%");
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Sunset Launcher\nComment=Install and play Sunset\nExec=\"{exe}\"\nIcon=sunset-launcher\nTerminal=false\nCategories=Game;\nStartupWMClass=sunset-launcher\n"
    );
    let files: [(&str, &[u8]); 3] = [
        ("icons/hicolor/scalable/apps/sunset-launcher.svg", include_bytes!("../assets/sunset-launcher.svg")),
        ("icons/hicolor/256x256/apps/sunset-launcher.png", include_bytes!("../assets/sunset-launcher.png")),
        ("applications/sunset-launcher.desktop", entry.as_bytes()),
    ];
    for (rel, bytes) in files {
        let path = data.join(rel);
        if std::fs::read(&path).ok().as_deref() != Some(bytes) {
            let _ = path.parent().map(std::fs::create_dir_all);
            let _ = std::fs::write(&path, bytes);
        }
    }
}

pub fn run() {
    #[cfg(target_os = "linux")]
    desktop_entry();
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
            // No system title bar: the top bar is the title bar (see `top_bar` and `window_controls`).
            titlebar: None,
            window_decorations: Some(WindowDecorations::Client),
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

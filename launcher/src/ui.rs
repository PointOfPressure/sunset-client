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

const BG: u32 = 0x0b0b0c;
const CARD: u32 = 0x141416;
const LINE: u32 = 0x242428;
const CHIP: u32 = 0x1b1b1e;
const TEXT: u32 = 0xececee;
const MUTED: u32 = 0xb4b4bb; // secondary text: quieter than TEXT, still easy to read on CARD and BG
const ACCENT: u32 = 0xdb5000; // Sunset orange, as on the site
const ACCENT_HI: u32 = 0xf26a12; // its hover
const ON_ACCENT: u32 = 0xffffff;
const GREEN: u32 = 0x6fcf97;
const AMBER: u32 = 0xe7b04a;
const INSET: u32 = 0x0f0f11; // a well set into a card (the changelog)
const RED: u32 = 0xef6b6b; // errors only; orange is the one accent
const SOFT: u32 = 0xd2d2d7; // labels and unselected tabs: a step under TEXT

// One type scale: the page's title, a card's heading, body text, small print.
const T_TITLE: f32 = 30.;
const T_HEAD: f32 = 15.;
const T_SMALL: f32 = 12.;

// One spacing scale for every page, so gutters, gaps and control heights line up across views.
const GUTTER: f32 = 24.; // window edge to content, left and right
const GAP: f32 = 16.; // between cards, columns and page blocks
const PAD: f32 = 16.; // inside cards and their rows
const CTRL_H: f32 = 36.; // buttons, top-bar chips and the tab pill
const BIG_H: f32 = 40.; // the Play row
const BAR_H: f32 = 68.; // the top bar, chips centred in it
const PANEL_H: f32 = 176.; // the changelog on Overview

// Corner radii: one scale for the whole window. Cards, menus and dialogs are R_CARD, everything you press is R_CTRL,
// small labels are R_SMALL. A shape nested close inside another (a tab in the tab strip, a menu row in its menu, the
// hover light inside a bordered button, the face in the account chip) takes the outer radius minus its inset, so both
// corners share one centre and read as the same curve. Round things (status dots, the switch, the progress bar) stay
// round.
const R_CARD: f32 = 10.;
const R_CTRL: f32 = 10.;
const R_SMALL: f32 = 6.;

/// The radius of a shape inset `by` px inside a corner of radius `outer`.
const fn nested(outer: f32, by: f32) -> f32 {
    if outer - by > 2. { outer - by } else { 2. }
}

/// The account face and the platform mark in their top-bar chips, and how far they sit from the chip's outer edge
/// (the chip's 1 px border included), which their corner radius follows.
const FACE: f32 = 22.;
const FACE_INSET: f32 = (CTRL_H - FACE) / 2.;

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

/// A button that does something; there are no disabled ones (what can't work isn't shown).
fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, primary: bool) -> gpui::Stateful<gpui::Div> {
    let (bg, fg, hover) = if primary { (ACCENT, ON_ACCENT, ACCENT_HI) } else { (CHIP, TEXT, 0x26262a) };
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
        .child(label.into())
}

/// A square button with only an icon; `tip` names it on hover.
fn icon_button(id: impl Into<ElementId>, kind: Icon, tip: &'static str, side: f32) -> gpui::Stateful<gpui::Div> {
    button(id, "", false).w(px(side)).h(px(side)).px_0().child(icon(kind, 18., TEXT)).tooltip(move |_, cx| cx.new(|_| Tip(tip.into())).into())
}

/// A hover label.
struct Tip(SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().px(px(8.)).py(px(4.)).rounded(px(R_SMALL)).bg(rgb(CHIP)).border_1().border_color(rgb(LINE)).font_family(BODY).font_weight(BODY_WEIGHT).text_size(px(12.)).text_color(rgb(TEXT)).child(self.0.clone())
    }
}

#[derive(Clone, Copy)]
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
    More,
}

/// The icons, drawn on one 24-unit grid with 1.75-unit strokes, round ends and joins and rounded corners, so the set
/// reads as one soft family at every size.
fn icon(kind: Icon, s: f32, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b: Bounds<gpui::Pixels>, _, window: &mut Window, _| {
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
                Icon::More => {
                    let mut p = PathBuilder::fill();
                    for cx_ in [6., 12., 18.] {
                        dot(&mut p, &at, cx_, 12., 1.7);
                    }
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
                Icon::Minimize => {
                    let mut p = stroke();
                    line(&mut p, &[(6., 12.), (18., 12.)]);
                    p
                }
                Icon::Maximize => {
                    let mut p = stroke();
                    rounded(&mut p, &at, &[(6., 6.), (18., 6.), (18., 18.), (6., 18.)], 2.5);
                    p
                }
                Icon::Restore => {
                    let mut p = stroke();
                    rounded(&mut p, &at, &[(5., 9.), (15., 9.), (15., 19.), (5., 19.)], 2.5);
                    // The window behind: only its top and right edges show.
                    p.move_to(at(9., 9.));
                    p.line_to(at(9., 7.5));
                    p.curve_to(at(11.5, 5.), at(9., 5.));
                    p.line_to(at(16.5, 5.));
                    p.curve_to(at(19., 7.5), at(19., 5.));
                    p.line_to(at(19., 12.5));
                    p.curve_to(at(16.5, 15.), at(19., 15.));
                    p.line_to(at(15., 15.));
                    p
                }
                Icon::Close => {
                    let mut p = stroke();
                    line(&mut p, &[(7., 7.), (17., 17.)]);
                    line(&mut p, &[(17., 7.), (7., 17.)]);
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

/// A filled dot of radius `r` units at (`x`, `y`).
fn dot(p: &mut PathBuilder, at: &dyn Fn(f32, f32) -> Point<Pixels>, x: f32, y: f32, r: f32) {
    for i in 0..24 {
        let a = i as f32 / 24. * std::f32::consts::TAU;
        let pt = at(x + r * a.cos(), y + r * a.sin());
        if i == 0 { p.move_to(pt) } else { p.line_to(pt) }
    }
    p.close();
}

/// One line of the changelog: its section as a fixed-width tag, then the text, wrapped. Important lines get an
/// accent tag and full-strength text.
fn change_row(c: &core::Change) -> gpui::Div {
    let kind = match c.kind.trim() {
        "" => "Note".to_string(),
        k => k.chars().take(9).collect(),
    };
    let mut line = c.text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > MAX_CHANGE_CHARS {
        line = line.chars().take(MAX_CHANGE_CHARS).collect::<String>() + "…";
    }
    let tag = div()
        .flex_none()
        .w(px(64.))
        .h(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(R_SMALL))
        .bg(rgb(if c.important { ACCENT } else { CHIP }))
        .text_size(px(11.))
        .font_family(BOLD).font_weight(BOLD_WEIGHT)
        .text_color(rgb(if c.important { ON_ACCENT } else { MUTED }))
        .child(kind);
    let body = div()
        .flex_1()
        .min_w(px(0.))
        .pt(px(1.))
        .text_size(px(13.))
        .line_height(px(18.))
        .text_color(rgb(if c.important { TEXT } else { SOFT }))
        .when(c.important, |d| d.font_family(BOLD).font_weight(BOLD_WEIGHT))
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
    let (clear, solid) = (Hsla::from(gpui::rgba(INSET << 8)), Hsla::from(rgb(INSET)));
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
    window.paint_quad(gpui::fill(at, rgb(0x3a3a40)).corner_radii(Corners::all(px(1.5))));
}

/// Horizon's mark (horizon-rust/horizon docs/media/brand/icon.svg), drawn on its 16-unit grid: an H on a dark tile
/// whose crossbar is the horizon, a violet line edge to edge, with the stems above it at full strength and their
/// reflection below at a third. The tile takes the chip's nested radius.
fn horizon_mark(s: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b: Bounds<gpui::Pixels>, _, window: &mut Window, _| {
            let u = b.size.width / 16.;
            let rect = |x: f32, y: f32, w: f32, h: f32| Bounds::new(point(b.origin.x + u * x, b.origin.y + u * y), size(u * w, u * h));
            // A hairline edge keeps the tile readable as a box on the dark chip it sits in.
            window.paint_quad(
                gpui::fill(b, rgb(0x18181b)).corner_radii(Corners::all(px(nested(R_CTRL, FACE_INSET)))).border_widths(px(1.)).border_color(rgb(0x38383e)),
            );
            for (y, colour) in [(3., gpui::rgba(0xecedf3ff)), (8., gpui::rgba(0xecedf35c))] {
                for x in [4., 10.] {
                    window.paint_quad(gpui::fill(rect(x, y, 2., 4.), colour));
                }
            }
            // Horizon's line is violet in its own brand; here it is neutral, so orange stays the window's one accent.
            window.paint_quad(gpui::fill(rect(0., 7., 16., 1.), rgb(0xa8a8b0)));
        },
    )
    .w(px(s))
    .h(px(s))
    .flex_none()
}

/// The Sunset mark (the site's and the game's): two half rings, drawn `w` wide and half as tall.
fn mark(w: f32, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b: Bounds<gpui::Pixels>, _, window: &mut Window, _| {
            // In the mark's own 432 x 216 units: centre x, baseline y, outer and inner radius, foot below the baseline.
            let k = b.size.width / 432.;
            let at = |x: f32, y: f32| point(b.origin.x + k * x, b.origin.y + k * y);
            for (cx_, cy, ro, ri, foot) in [(216., 216., 216., 167., 0.), (216., 211., 121., 72., 5.)] {
                let arc = |r: f32| (0..=32).map(move |i| {
                    let a = std::f32::consts::PI * (1. + i as f32 / 32.);
                    (cx_ + r * a.cos(), cy + r * a.sin())
                });
                let mut p = PathBuilder::fill();
                p.move_to(at(cx_ - ro, cy + foot));
                for (x, y) in arc(ro) {
                    p.line_to(at(x, y));
                }
                p.line_to(at(cx_ + ro, cy + foot));
                p.line_to(at(cx_ + ri, cy + foot));
                for (x, y) in arc(ri).collect::<Vec<_>>().into_iter().rev() {
                    p.line_to(at(x, y));
                }
                p.line_to(at(cx_ - ri, cy + foot));
                p.close();
                if let Ok(path) = p.build() {
                    window.paint_path(path, rgb(color));
                }
            }
        },
    )
    .w(px(w))
    .h(px(w / 2.))
    .flex_none()
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

fn row(label: impl Into<SharedString>, value: impl IntoElement) -> gpui::Div {
    div().flex().items_center().justify_between().h(px(32.)).child(text(label, 13., MUTED)).child(value)
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

fn window_label(w: &str) -> String {
    if w == "fullscreen" { "Fullscreen".into() } else { w.replace('x', " × ") }
}

/// A page's title row, the same height on every page whether or not it has a button on the right.
fn page_head(title: &str, action: Option<gpui::AnyElement>) -> gpui::Div {
    div().flex().flex_none().items_center().justify_between().h(px(CTRL_H)).child(bold(title.to_string(), T_TITLE)).children(action)
}

/// The top bar's chips (platform, instance, account) share this shape.
fn top_chip(id: impl Into<ElementId>) -> gpui::Stateful<gpui::Div> {
    fx::control(id, 0x26262a, nested(R_CTRL, 1.))
        .flex()
        .items_center()
        .gap(px(8.))
        .h(px(CTRL_H))
        .px(px(14.))
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
        fx::control(id, hover, R_CTRL)
            .w(px(46.))
            .h(px(CTRL_H))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(R_CTRL))
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
            .gap(px(2.))
            .ml(px(8.))
            .occlude()
            .child(ctl("win-min", Icon::Minimize, WindowControlArea::Min, 0x26262a).when(own, |d| d.on_click(|_, window, _| window.minimize_window())))
            .child(ctl("win-max", max, WindowControlArea::Max, 0x26262a).when(own, |d| d.on_click(|_, window, _| window.zoom_window())))
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
        let left = group()
            .child(self.dropdown(Menu::Platform, "Horizon", cx))
            .child(self.dropdown(Menu::Instance, if has { "Sunset" } else { "No instance" }, cx))
            .child({
                let shown: Vec<_> = tabs.into_iter().filter(|(t, _)| has || *t == Tab::Overview || *t == Tab::Settings).collect();
                // Tabs are sized here from their labels, so the pill that slides under them knows where each one is
                // in the same frame.
                // Inside the strip's 1 px border and 3 px padding.
                let tab_h = CTRL_H - 8.;
                let tab_r = nested(R_CTRL, 4.);
                let mut x = 0.;
                let spans: Vec<(f32, f32)> = shown
                    .iter()
                    .map(|(_, label)| {
                        let w = (label_width(window, label, 13.) + 36.).ceil();
                        x += w + 2.;
                        (x - w - 2., w)
                    })
                    .collect();
                let widths: Vec<f32> = spans.iter().map(|s| s.1).collect();
                let pill = shown.iter().position(|(t, _)| *t == self.tab).map(|at| {
                    div().absolute().top(px(3.)).h(px(tab_h)).rounded(px(tab_r)).bg(rgb(0x2a2a2e)).with_spring(
                        "tab-pill",
                        SpringAnimation::new(fx::SLIDE).to(at as f32).with_epsilon(0.002),
                        move |d, p| {
                            let (left, w) = fx::pill_span(&spans, p);
                            d.left(px(3. + left)).w(px(w))
                        },
                    )
                });
                div().relative().flex().p(px(3.)).gap(px(2.)).rounded(px(R_CTRL)).bg(rgb(CHIP)).border_1().border_color(rgb(LINE)).children(pill).children(shown.into_iter().zip(widths).map(|((t, label), w)| {
                    let on = self.tab == t;
                    let id = ElementId::from(label);
                    let lit = on || fx::hovered(&id);
                    fx::pressable(id, None, tab_r)
                        .w(px(w))
                        .h(px(tab_h))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .text_size(px(13.))
                        .child(div().child(label).with_spring(
                            "tab-text",
                            SpringAnimation::new(fx::HOVER).to(if lit { 1f32 } else { 0. }).with_epsilon(0.004),
                            |d, t| d.text_color(fx::mix(MUTED, TEXT, t)),
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
                }))
            });
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(BAR_H))
            .gap(px(8.))
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
            .child(div().flex_1())
            .child(group().child(self.account_chip(cx)))
            .children(window_controls(window))
    }

    /// A top-bar chip that toggles `menu`; its list drops down under it (the backdrop is in `menu_popup`).
    fn dropdown(&self, menu: Menu, label: &'static str, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let id = if menu == Menu::Platform { "platform" } else { "instance" };
        let chip = top_chip(id)
            // The platform chip leads with the platform's mark, inset like the account chip's face.
            .when(menu == Menu::Platform, |d| d.pl(px(FACE_INSET - 1.)).child(horizon_mark(FACE)))
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
                    Some(_) => pop.child(self.menu_row("i-sunset", checked("Sunset", true), cx, |this, _| this.tab = Tab::Overview)),
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
            return top_chip("sign-in").child(text("Sign in", 13., TEXT)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_sign_in(cx))).into_any_element();
        };
        let head = self.faces.iter().find(|(u, _)| u == &a.uuid).map(|(_, f)| face(f, FACE).into_any_element()).unwrap_or_else(|| div().w(px(FACE)).h(px(FACE)).rounded(px(nested(R_CTRL, FACE_INSET))).bg(rgb(0x3a3a40)).into_any_element());
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
        fx::control(id, 0x26262a, nested(R_CARD, 5.))
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
                .child(div().h(px(6.)).w_full().rounded_full().bg(rgb(0x2a2a2e)).child(div().h_full().rounded_full().bg(rgb(ACCENT)).w(gpui::relative(if j.frac.is_some() { frac } else { 1. })).when(j.frac.is_none(), |d| d.opacity(0.35)))),
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
                .gap(px(8.))
                .child(
                    if self.playing { div().id("play") } else { fx::control("play", ACCENT_HI, R_CTRL) }
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(10.))
                        .w(px(200.))
                        .h(px(BIG_H))
                        .rounded(px(R_CTRL))
                        .bg(rgb(ACCENT))
                        .when(self.playing, |d| d.opacity(0.6))
                        .when(!self.playing, |d| d.cursor_pointer())
                        .child(icon(Icon::Play, 20., ON_ACCENT))
                        .child(div().text_size(px(16.)).font_family(BOLD).font_weight(BOLD_WEIGHT).text_color(rgb(ON_ACCENT)).child(if self.playing { "Playing" } else { "Play" }))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.play(cx))),
                )
                .when_some(update, |d, v| d.child(button("update", format!("Update to {v}"), false).h(px(BIG_H)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_install(false, cx)))))
                .child(icon_button("open-folder", Icon::Folder, "Open folder", BIG_H).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    let p = this.paths.instance();
                    this.open(p, cx)
                })))
                .child(div().flex_1())
                .child(self.with_menu(Menu::More, icon_button("more", Icon::More, "More", BIG_H).on_click(cx.listener(|this, ev: &ClickEvent, _, cx| this.toggle_menu(Menu::More, ev, cx))), true, cx))
                .into_any_element(),
        };
        let title = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(bold(format!("Sunset {}", m.version), 20.))
                    .child(div().flex().items_center().gap(px(6.)).child(div().w(px(7.)).h(px(7.)).rounded_full().bg(rgb(dot))).child(text(status, 12., dot))),
            )
            .child(text(format!("Minecraft {}", m.horizon.game), 13., MUTED));
        // The Play row sits at the bottom so it lines up with the right card's buttons.
        // min_w(0): the changelog's long lines wrap inside the card instead of widening it past the window.
        let left = card()
            .flex_1()
            .min_w(px(0.))
            .p(px(PAD))
            .flex()
            .flex_col()
            .justify_between()
            .gap(px(GAP))
            .child(div().flex().flex_col().min_w(px(0.)).gap(px(GAP)).child(self.changelog(m)).child(title))
            .child(play_area);
        let plugins = self.mods.iter().filter(|(id, _)| id != "sunset").count();
        let right = card()
            .w(px(320.))
            .flex_none()
            .p(px(PAD))
            .flex()
            .flex_col()
            .child(bold("Installation", 16.))
            .child(row("Platform", text("Horizon", 13., TEXT)))
            .child(row("Runtime", text(m.short_pin().to_string(), 13., TEXT)))
            .child(row("Sunset", text(m.version.clone(), 13., TEXT)))
            .child(div().h(px(1.)).bg(rgb(LINE)).my(px(12.)))
            .child(bold("Configuration", 16.))
            .child(row("Profiles", text(format!("{} saved", self.profiles.len()), 13., TEXT)))
            .child(row("Plugins", text(if plugins == 0 { "Built in".to_string() } else { format!("{plugins} built in") }, 13., TEXT)))
            .child(div().flex_1().min_h(px(GAP)))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button("to-plugins", "Plugins", false).h(px(BIG_H)).flex_1().on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.tab = Tab::Plugins;
                        cx.notify();
                    })))
                    .child(button("to-profiles", "Profiles", false).h(px(BIG_H)).flex_1().on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.tab = Tab::Profiles;
                        this.reload_local();
                        cx.notify();
                    }))),
            );
        let dir = self.paths.instance();
        div()
            .flex()
            .flex_col()
            .gap(px(GAP))
            .child(page_head("Sunset", None))
            .when_some(self.import_prompt(cx), |d, p| d.child(p))
            .child(div().flex().gap(px(GAP)).child(left).child(right))
            .child(
                fx::control("instance-dir", 0x18181b, nested(R_CARD, 1.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE))
                    .rounded(px(R_CARD))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .h(px(48.))
                    .px(px(PAD))
                    .cursor_pointer()
                    .child(icon(Icon::Folder, 16., MUTED))
                    .child(text("Instance directory", 13., TEXT))
                    .child(div().flex_1())
                    .child(text(dir.display().to_string(), 12., MUTED))
                    .child(icon(Icon::Arrow, 16., MUTED))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(dir.clone(), cx))),
            )
            .into_any_element()
    }

    /// The installed version's changelog, from its signed manifest, where the banner was and as large. The list
    /// scrolls inside the panel; edge fades and a thin thumb show there is more (`scroll_cues`).
    fn changelog(&self, m: &Manifest) -> gpui::Div {
        let changes: Vec<&core::Change> = m.changelog.iter().filter(|c| !c.text.trim().is_empty()).take(MAX_CHANGES).collect();
        let count = match changes.len() {
            0 => String::new(),
            1 => "1 change".into(),
            n => format!("{n} changes"),
        };
        let panel = div()
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .w_full()
            .min_w(px(0.))
            .h(px(PANEL_H))
            .rounded(px(R_CTRL))
            .bg(rgb(INSET))
            .border_1()
            .border_color(rgb(LINE))
            .overflow_hidden()
            .child(div().flex_none().flex().items_center().justify_between().h(px(40.)).px(px(14.)).child(bold(format!("What's new in {}", m.version), 13.)).child(text(count, 12., MUTED)));
        if changes.is_empty() {
            return panel.child(div().flex_1().flex().items_center().justify_center().child(text(format!("Sunset {} came without a changelog.", m.version), 12., MUTED)));
        }
        let handle = self.log_scroll.clone();
        panel
            .child(
                div()
                    .id("changelog")
                    .flex_1()
                    .w_full()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .track_scroll(&self.log_scroll)
                    .child(div().flex_none().w_full().flex().flex_col().gap(px(10.)).px(px(14.)).pb(px(14.)).children(changes.into_iter().map(change_row))),
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
        div().flex().flex_col().gap(px(GAP)).size_full().child(page_head("Sunset", None)).child(card().flex_1().min_h(px(300.)).flex().items_center().justify_center().child(inner))
    }

    /// Settings: the instance's (when there is one) on the left, the launcher's and About on the right.
    fn settings_view(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let launcher = self.launcher_sections(cx);
        let page = div().flex().flex_col().gap(px(GAP)).child(page_head("Settings", None));
        let Some(m) = self.installed.as_ref() else {
            let (main, about) = launcher;
            return page.child(div().flex().items_start().gap(px(GAP)).child(main).child(about)).into_any_element();
        };
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
        let (inst, logs) = (self.paths.instance(), self.paths.logs());
        let left = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(section("Instance", vec![
                setting_row("Name", text("Sunset", 13., TEXT), false),
                setting_row("Sunset version", version, false),
                setting_row("Runtime", text(format!("Horizon {} ({})", m.short_pin(), m.horizon.tag), 13., TEXT), false),
                setting_row(
                    "Window size",
                    self.with_menu(Menu::Window, button("s-window", window_label(&self.settings.window), false).on_click(cx.listener(|this, ev: &ClickEvent, _, cx| this.toggle_menu(Menu::Window, ev, cx))), true, cx),
                    true,
                ),
            ]))
            .child(section("Updates", vec![
                setting_row("Install new Sunset releases automatically", toggle, false),
                setting_row("Check for updates", button("s-check", "Check", false).on_click(cx.listener(|this, _: &ClickEvent, _, _| this.check(false))), true),
            ]))
            .child(section("Files", vec![
                setting_row("Instance directory", button("s-open", "Open folder", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(inst.clone(), cx))), false),
                setting_row("Logs", button("s-logs", "Open logs", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(logs.clone(), cx))), false),
                setting_row("Repair installation (check and download again)", button("s-repair", "Repair", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_install(true, cx))), true),
            ]));
        let (main, about) = launcher;
        page.when_some(self.progress(cx), |d, p| d.child(card().p(px(PAD)).child(p)))
            .child(div().flex().items_start().gap(px(GAP)).child(left).child(main.child(about)))
            .into_any_element()
    }

    /// The launcher's own sections (data folder, release source, horizon-launcher, import) and About, as two columns.
    fn launcher_sections(&self, cx: &mut Context<Self>) -> (gpui::Div, gpui::Div) {
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
        let key = include_str!("../sunset.pub").lines().next().unwrap_or("").rsplit(' ').next().unwrap_or("").to_string();
        let main = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(section("Launcher", vec![
                setting_row("Data folder", button("l-data", "Open folder", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open(root.clone(), cx))), false),
                setting_row("Release source", text(source, T_SMALL, MUTED), false),
                setting_row("horizon-launcher (sign-in, install, play)", text(cli.unwrap_or_else(|| "comes with the first install".into()), 13., TEXT), true),
            ]))
            .child(section("Import Sunset configs and profiles from Horizon", import_rows));
        let about = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(8.)).child(bold("About", 15.)).child(
            card()
                .flex()
                .flex_col()
                .child(setting_row("Sunset launcher", text(core::VERSION, 13., TEXT), false))
                .child(setting_row("Signing key", text(key, T_SMALL, MUTED), false))
                .child(setting_row("Platform", text(core::PLATFORM, 13., TEXT), false))
                .child(setting_row("Licence", text("PolyForm Strict 1.0.0", 13., TEXT), false))
                .child(div().p(px(PAD)).flex().flex_col().gap(px(8.)).child(text("Installs the signed prebuilt Sunset client for the Horizon Nightly it was built and tested on, and plays it through the stock horizon-launcher, which keeps your Microsoft sign-in.", T_SMALL, MUTED)).child(text("Horizon is LGPL-3.0-or-later; the exact source each prebuilt is built from is published with it. Not affiliated with Mojang or Microsoft.", T_SMALL, MUTED))),
        );
        (main, about)
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

    fn footer(&self) -> impl IntoElement + use<> {
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .flex_none()
            .h(px(48.))
            .child(mark(22., ACCENT))
            .child(text(format!("Launcher {}", core::VERSION), 12., MUTED))
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
                self.list_view("Plugins", "Nothing built into this version yet.".into(), items, None, cx).into_any_element()
            }
            Tab::Profiles => {
                let items = self.profiles.iter().map(|p| (p.clone(), String::new())).collect();
                let empty = "No saved profiles yet. Save one in game from Sunset's menu (Right Shift).".to_string();
                self.list_view("Profiles", empty, items, Some(self.paths.sunset_data().join("profiles")), cx).into_any_element()
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
            .child(self.top_bar(window, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .mt(px(8.))
                    .flex()
                    .flex_col()
                    .child(div().id("page").flex_1().min_h(px(0.)).overflow_y_scroll().track_scroll(&self.page_scroll).child(page))
                    .child(fx::smooth_scroll("page", &self.page_scroll)),
            )
            .when_some(note, |d, n| d.child(n.mt(px(GAP))))
            .child(self.footer())
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

// Anti Vanish's Staff check (the behaviour of Blackout's Staff Check, written fresh): staff testing a player for
// anti-knockback push them with a velocity packet (SetEntityMotion for you) when nothing in the game would. Each push
// waits Staff window ticks; then what came within that many ticks of it (a hit, an explosion, a teleport) and what was
// around when it came (an attacker in reach, a projectile, a hook, riding, own motion) explain it, or nothing does.
// Pure logic: vanish.rs feeds it from the game.

/// What would make the server push you, each true when it was there for a push.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Around {
    pub(super) hurt: bool,
    pub(super) explosion: bool,
    pub(super) attacker: bool,
    pub(super) projectile: bool,
    pub(super) riding: bool,
    pub(super) own: bool,
    pub(super) teleport: bool,
}

/// The causes in the order they are checked, then "unexplained" (the indexes of Staff::counts).
pub(super) const CAUSES: [&str; 8] =
    ["hurt", "explosion", "attacker in reach", "projectile or hook", "riding", "own motion", "teleport", "unexplained"];
pub(super) const UNEXPLAINED: usize = 7;

impl Around {
    /// The first cause there (an index into CAUSES); None: nothing explains the push.
    pub(super) fn cause(&self) -> Option<usize> {
        [self.hurt, self.explosion, self.attacker, self.projectile, self.riding, self.own, self.teleport].iter().position(|b| *b)
    }
}

/// A push for you: the tick it came, its velocity (blocks a tick) and what was around then.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Push {
    pub(super) tick: u64,
    pub(super) velocity: [f64; 3],
    pub(super) around: Around,
}

impl Push {
    pub(super) fn across(&self) -> f64 { self.velocity[0].hypot(self.velocity[2]) }
    fn size(&self) -> f64 { (self.velocity.iter().map(|v| v * v).sum::<f64>()).sqrt() }
}

pub(super) enum Event {
    Hurt,
    Blast,
    Teleport,
}

/// The smallest push that counts (blocks a tick): a zero one only stops you.
pub(super) const MIN_PUSH: f64 = 0.05;
/// How near (blocks, centre to centre) a mob or player must be to have hit you, a projectile to have hit you, an
/// explosion to have pushed you, and a fishing hook to be pulling you.
pub(super) const REACH: f64 = 6.0;
pub(super) const PROJECTILE_RANGE: f64 = 8.0;
pub(super) const BLAST_RANGE: f64 = 16.0;
pub(super) const HOOK_RANGE: f64 = 40.0;
// Events are kept this many ticks (the window is at most 20); pushes waiting at most this many.
const KEEP: u64 = 100;
const MAX_WAITING: usize = 64;

#[derive(Default)]
pub(super) struct Staff {
    waiting: Vec<Push>,
    hurts: Vec<u64>,
    blasts: Vec<u64>,
    teleports: Vec<u64>,
    health: Option<f32>,
    last_alert: Option<u64>,
    /// The pushes judged since the connection, by cause (CAUSES).
    pub(super) counts: [u32; 8],
    /// Unexplained pushes said, then held back (HELD).
    pub(super) alerts: u32,
    pub(super) held: [u32; 3],
}

/// Why an unexplained push was not said.
pub(super) const HELD: [&str; 3] = ["no Staff names on", "cooldown", "notifications off"];

impl Staff {
    pub(super) fn event(&mut self, e: Event, tick: u64) {
        let list = match e {
            Event::Hurt => &mut self.hurts,
            Event::Blast => &mut self.blasts,
            Event::Teleport => &mut self.teleports,
        };
        list.retain(|t| tick.saturating_sub(*t) <= KEEP);
        list.push(tick);
    }

    /// Your health from the server: a drop is a hurt (some servers send no DamageEvent).
    pub(super) fn health(&mut self, health: f32, tick: u64) {
        if self.health.is_some_and(|before| health < before) {
            self.event(Event::Hurt, tick);
        }
        self.health = Some(health);
    }

    /// A push for you, judged once its window has passed. False: too small to count.
    pub(super) fn push(&mut self, p: Push) -> bool {
        if p.size() < MIN_PUSH || self.waiting.len() >= MAX_WAITING {
            return false;
        }
        self.waiting.push(p);
        true
    }

    /// Off: nothing waits.
    pub(super) fn clear(&mut self) { self.waiting.clear(); }

    /// The pushes whose window has passed at `now`, each with its cause (None: unexplained), counted.
    pub(super) fn settle(&mut self, now: u64, window: u64) -> Vec<(Push, Option<usize>)> {
        let near = |list: &[u64], t: u64| list.iter().any(|e| e.abs_diff(t) <= window);
        let (due, waiting): (Vec<Push>, Vec<Push>) = self.waiting.drain(..).partition(|p| now >= p.tick + window);
        self.waiting = waiting;
        due.into_iter()
            .map(|mut p| {
                p.around.hurt |= near(&self.hurts, p.tick);
                p.around.explosion |= near(&self.blasts, p.tick);
                p.around.teleport |= near(&self.teleports, p.tick);
                let cause = p.around.cause();
                self.counts[cause.unwrap_or(UNEXPLAINED)] += 1;
                (p, cause)
            })
            .collect()
    }

    /// True (and the time noted) when no alert came in the last `cooldown` ticks.
    pub(super) fn may_alert(&mut self, now: u64, cooldown: u64) -> bool {
        if self.last_alert.is_some_and(|t| now < t + cooldown) {
            self.held[1] += 1;
            return false;
        }
        self.last_alert = Some(now);
        self.alerts += 1;
        true
    }
}

fn bare(kind: &str) -> &str { kind.strip_prefix("minecraft:").unwrap_or(kind) }

/// Entities that push you when they hit: projectiles, wind charges, rockets.
pub(super) fn projectile(kind: &str) -> bool {
    matches!(
        bare(kind),
        "arrow" | "spectral_arrow" | "trident" | "wind_charge" | "breeze_wind_charge" | "firework_rocket" | "snowball"
            | "egg" | "ender_pearl" | "fireball" | "small_fireball" | "dragon_fireball" | "wither_skull" | "shulker_bullet"
            | "llama_spit" | "splash_potion" | "lingering_potion" | "potion"
    )
}

/// Entities that never push anyone: items, orbs, decorations, markers, displays.
pub(super) fn inert(kind: &str) -> bool {
    matches!(
        bare(kind),
        "item" | "experience_orb" | "item_frame" | "glow_item_frame" | "painting" | "marker" | "block_display"
            | "item_display" | "text_display" | "interaction" | "area_effect_cloud" | "falling_block" | "armor_stand"
            | "leash_knot" | "eye_of_ender" | "experience_bottle" | "lightning_bolt"
    )
}

/// The other entities about (kind, distance): (an attacker in reach, a projectile or hook near).
pub(super) fn entities<'a>(near: impl IntoIterator<Item = (&'a str, f64)>) -> (bool, bool) {
    let (mut attacker, mut thrown) = (false, false);
    for (kind, d) in near {
        if bare(kind) == "fishing_bobber" {
            thrown |= d <= HOOK_RANGE;
        } else if projectile(kind) {
            thrown |= d <= PROJECTILE_RANGE;
        } else if !inert(kind) {
            attacker |= d <= REACH;
        }
    }
    (attacker, thrown)
}

/// Staff check's settings in the menu, in either Mode (HorizonSMP takes its notifications' interval for the
/// cooldown). None: not one of them.
pub(super) fn shown(name: &str, on: bool, horizon: bool) -> Option<bool> {
    match name {
        "Staff check" => Some(true),
        "Staff window" | "Staff names" => Some(on),
        "Staff cooldown" => Some(on && !horizon),
        _ => None,
    }
}

/// Staff names as a list (comma separated; empty: none, so always alert).
pub(super) fn names(list: &str) -> Vec<&str> { list.split(',').map(str::trim).filter(|n| !n.is_empty()).collect() }

#[cfg(test)]
mod tests {
    use super::*;

    fn push(tick: u64, around: Around) -> Push { Push { tick, velocity: [0.3, 0.36, 0.0], around } }

    #[test]
    fn a_hit_within_the_window_explains_the_push_either_side() {
        let mut s = Staff::default();
        s.event(Event::Hurt, 98);
        assert!(s.push(push(100, Around::default())));
        assert!(s.settle(104, 5).is_empty(), "waits for its window");
        assert_eq!(s.settle(105, 5)[0].1, Some(0));
        // A hit 3 ticks after the push.
        s.push(push(200, Around::default()));
        s.event(Event::Hurt, 203);
        assert_eq!(s.settle(205, 5)[0].1, Some(0));
        // A hit 8 ticks before is outside a window of 5.
        s.event(Event::Hurt, 292);
        s.push(push(300, Around::default()));
        assert_eq!(s.settle(305, 5)[0].1, None);
        assert_eq!((s.counts[0], s.counts[UNEXPLAINED]), (2, 1));
    }

    #[test]
    fn a_health_drop_is_a_hurt_but_a_rise_is_not() {
        let mut s = Staff::default();
        s.health(20.0, 1);
        s.health(20.0, 9);
        s.push(push(10, Around::default()));
        assert_eq!(s.settle(20, 5)[0].1, None);
        s.health(16.0, 30);
        s.push(push(31, Around::default()));
        assert_eq!(s.settle(40, 5)[0].1, Some(0));
    }

    #[test]
    fn explosion_and_teleport_explain() {
        let mut s = Staff::default();
        s.event(Event::Blast, 50);
        s.push(push(50, Around::default()));
        s.event(Event::Teleport, 80);
        s.push(push(81, Around::default()));
        let out = s.settle(90, 3);
        assert_eq!((out[0].1, out[1].1), (Some(1), Some(6)));
    }

    #[test]
    fn what_was_around_explains_and_nothing_does_not() {
        assert_eq!(Around { attacker: true, ..Around::default() }.cause(), Some(2));
        assert_eq!(Around { riding: true, ..Around::default() }.cause(), Some(4));
        assert_eq!(Around { own: true, ..Around::default() }.cause(), Some(5));
        assert_eq!(Around { hurt: true, attacker: true, ..Around::default() }.cause(), Some(0));
        assert_eq!(Around::default().cause(), None);
    }

    #[test]
    fn attackers_projectiles_and_hooks_by_distance() {
        assert_eq!(entities([("minecraft:zombie", 3.0)]), (true, false));
        assert_eq!(entities([("minecraft:player", 5.9)]), (true, false));
        assert_eq!(entities([("minecraft:player", 9.0)]), (false, false));
        assert_eq!(entities([("minecraft:item", 1.0), ("minecraft:armor_stand", 1.0), ("minecraft:experience_orb", 0.5)]), (false, false));
        assert_eq!(entities([("minecraft:wind_charge", 7.5)]), (false, true));
        assert_eq!(entities([("minecraft:arrow", 12.0)]), (false, false));
        assert_eq!(entities([("minecraft:fishing_bobber", 30.0)]), (false, true));
        assert_eq!(entities([]), (false, false));
    }

    #[test]
    fn small_pushes_do_not_count_and_the_queue_is_bounded() {
        let mut s = Staff::default();
        assert!(!s.push(Push { tick: 1, velocity: [0.0, 0.0, 0.0], around: Around::default() }));
        assert!(!s.push(Push { tick: 1, velocity: [0.02, -0.03, 0.01], around: Around::default() }));
        for t in 0..80 {
            s.push(push(t, Around::default()));
        }
        assert_eq!(s.settle(1000, 5).len(), MAX_WAITING);
        assert!(s.settle(2000, 5).is_empty());
    }

    #[test]
    fn cooldown() {
        let mut s = Staff::default();
        assert!(s.may_alert(100, 200));
        assert!(!s.may_alert(299, 200));
        assert!(s.may_alert(300, 200));
        assert!(s.may_alert(300, 0) && s.may_alert(300, 0));
        assert_eq!((s.alerts, s.held[1]), (4, 1));
    }

    #[test]
    fn settings_show_in_both_modes() {
        for horizon in [false, true] {
            assert_eq!(shown("Staff check", false, horizon), Some(true));
            assert_eq!(shown("Staff window", false, horizon), Some(false));
            assert_eq!(shown("Staff window", true, horizon), Some(true));
            assert_eq!(shown("Staff names", true, horizon), Some(true));
            assert_eq!(shown("Mode", true, horizon), None);
        }
        assert_eq!((shown("Staff cooldown", true, false), shown("Staff cooldown", true, true)), (Some(true), Some(false)));
    }

    #[test]
    fn names_list() {
        assert_eq!(names(" Mod_1, ,admin ,"), vec!["Mod_1", "admin"]);
        assert!(names("  ").is_empty());
    }
}

//! Idle life: a sprite that has been idle for a while takes short strolls sideways and comes back
//! home. "Home" is the user's saved position; strolls never change it.

use crate::windows::positions::Pos;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Idle time before the first stroll is allowed.
pub const MIN_IDLE: Duration = Duration::from_secs(20);
/// Stroll distance range (px) and duration.
pub const STROLL_MIN: i32 = 40;
pub const STROLL_MAX: i32 = 120;
pub const STROLL_STEPS: u32 = 72;
pub const STROLL_STEP: Duration = Duration::from_millis(25);
/// Hop height (px) during a stroll.
pub const HOP: f64 = 4.0;
/// Reports from the window (onMoved) are ignored this long after a stroll ends.
pub const SETTLE: Duration = Duration::from_millis(600);

#[derive(Debug, Clone, PartialEq)]
pub struct Wander {
    pub home: Pos,
    pub away: bool,
    pub strolling: bool,
    pub next_at: Instant,
    pub ignore_reports_until: Instant,
}

#[derive(Default)]
pub struct Wanderers { map: HashMap<String, Wander> }

/// What the ticker decided for one window.
#[derive(Debug, PartialEq)]
pub enum Decision { Nothing, Stroll { from: Pos, to: Pos }, GoHome { from: Pos, to: Pos } }

impl Wanderers {
    pub fn get(&self, label: &str) -> Option<&Wander> { self.map.get(label) }
    pub fn remove(&mut self, label: &str) { self.map.remove(label); }
    pub fn is_strolling(&self, label: &str) -> bool { self.map.get(label).is_some_and(|w| w.strolling) }
    pub fn ignores_reports(&self, label: &str, now: Instant) -> bool {
        self.map.get(label).is_some_and(|w| w.strolling || now < w.ignore_reports_until)
    }

    /// The user moved the window: that is the new home and we are no longer away.
    pub fn rehome(&mut self, label: &str, pos: Pos, now: Instant) {
        let w = self.map.entry(label.into()).or_insert_with(|| Wander::new(pos, now));
        w.home = pos; w.away = false; w.next_at = now + Self::jitter(15, 40, now);
    }

    /// Called every second per window. `idle_for` is how long the agent has been in a strollable
    /// state (None when it is not), `pos` the current collapsed origin, `screen_w` the monitor width.
    pub fn tick(&mut self, label: &str, idle_for: Option<Duration>, min_idle: Duration, pos: Pos, screen_w: i32, sprite_w: i32, now: Instant) -> Decision {
        let w = self.map.entry(label.into()).or_insert_with(|| Wander::new(pos, now));
        if w.strolling { return Decision::Nothing; }
        match idle_for {
            None => {
                if w.away { Decision::GoHome { from: pos, to: w.home } } else { w.home = pos; Decision::Nothing }
            }
            Some(d) => {
                if d < min_idle || now < w.next_at { if !w.away { w.home = pos; } return Decision::Nothing; }
                if w.away { return Decision::GoHome { from: pos, to: w.home }; }
                w.home = pos;
                let to = pick_destination(w.home, screen_w, sprite_w, Self::rand_i32(STROLL_MIN, STROLL_MAX, now), Self::rand_sign(now));
                Decision::Stroll { from: pos, to }
            }
        }
    }

    pub fn stroll_started(&mut self, label: &str) { if let Some(w) = self.map.get_mut(label) { w.strolling = true; } }
    pub fn stroll_finished(&mut self, label: &str, away: bool, now: Instant) {
        if let Some(w) = self.map.get_mut(label) {
            w.strolling = false; w.away = away;
            w.ignore_reports_until = now + SETTLE;
            w.next_at = now + if away { Self::jitter(3, 6, now) } else { Self::jitter(15, 40, now) };
        }
    }

    fn jitter(lo: u64, hi: u64, now: Instant) -> Duration { Duration::from_secs(Self::rand_i32(lo as i32, hi as i32, now) as u64) }
    fn rand_i32(lo: i32, hi: i32, _now: Instant) -> i32 {
        use rand::Rng;
        rand::rng().random_range(lo..=hi)
    }
    fn rand_sign(_now: Instant) -> i32 { use rand::Rng; if rand::rng().random_bool(0.5) { 1 } else { -1 } }
}

impl Wander {
    fn new(home: Pos, now: Instant) -> Self {
        Self { home, away: false, strolling: false, next_at: now, ignore_reports_until: now }
    }
}

/// Destination for a stroll of `dist` px in direction `sign`, kept on screen; flips direction if
/// it would leave the screen.
pub fn pick_destination(home: Pos, screen_w: i32, sprite_w: i32, dist: i32, sign: i32) -> Pos {
    let max_x = (screen_w - sprite_w).max(0);
    let try_x = home.x + sign * dist;
    let x = if (0..=max_x).contains(&try_x) { try_x } else { (home.x - sign * dist).clamp(0, max_x) };
    Pos { x, y: home.y }
}

/// Ease-in-out position at step `i` of `steps`, with a small hop.
pub fn step_position(from: Pos, to: Pos, i: u32, steps: u32) -> Pos {
    let t = (i as f64 / steps as f64).clamp(0.0, 1.0);
    let e = if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 };
    // three gentle hops per stroll: sin² has no cusps at the landings, and the hops fade in/out with the stroll
    let phase = (std::f64::consts::PI * t * 3.0).sin();
    let envelope = (std::f64::consts::PI * t).sin();
    let hop = phase * phase * envelope * HOP;
    Pos { x: (from.x as f64 + (to.x - from.x) as f64 * e).round() as i32, y: (from.y as f64 + (to.y - from.y) as f64 * e - hop).round() as i32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn destination_stays_on_screen_and_flips_direction_at_the_edge() {
        assert_eq!(pick_destination(Pos { x: 500, y: 100 }, 1440, 192, 100, 1), Pos { x: 600, y: 100 });
        assert_eq!(pick_destination(Pos { x: 500, y: 100 }, 1440, 192, 100, -1), Pos { x: 400, y: 100 });
        assert_eq!(pick_destination(Pos { x: 1200, y: 100 }, 1440, 192, 100, 1), Pos { x: 1100, y: 100 }, "would leave right edge: go left");
        assert_eq!(pick_destination(Pos { x: 20, y: 100 }, 1440, 192, 100, -1), Pos { x: 120, y: 100 }, "would leave left edge: go right");
    }

    #[test] fn steps_start_at_from_end_at_to_and_hop_in_between() {
        let (a, b) = (Pos { x: 0, y: 50 }, Pos { x: 100, y: 50 });
        assert_eq!(step_position(a, b, 0, 72), a);
        assert_eq!(step_position(a, b, 72, 72), b);
        let mid = step_position(a, b, 36, 72);
        assert_eq!(mid.x, 50);
        let hop = step_position(a, b, 12, 72);
        assert!(hop.y < 50, "hop lifts the sprite");
    }

    #[test] fn configured_threshold_is_respected() {
        let now = Instant::now();
        let mut ws = Wanderers::default();
        let p = Pos { x: 300, y: 300 };
        assert_eq!(ws.tick("w", Some(Duration::from_secs(5)), Duration::from_secs(60), p, 1440, 192, now), Decision::Nothing);
        assert!(matches!(ws.tick("w", Some(Duration::from_secs(5)), Duration::from_secs(5), p, 1440, 192, now), Decision::Stroll { .. }));
    }

    #[test] fn no_stroll_before_min_idle_or_while_busy() {
        let now = Instant::now();
        let mut ws = Wanderers::default();
        assert_eq!(ws.tick("w", None, MIN_IDLE, Pos { x: 10, y: 10 }, 1440, 192, now), Decision::Nothing);
        assert_eq!(ws.tick("w", Some(Duration::from_secs(5)), MIN_IDLE, Pos { x: 10, y: 10 }, 1440, 192, now), Decision::Nothing);
    }

    #[test] fn strolls_when_idle_then_returns_home_and_rehomes_on_drag() {
        let now = Instant::now();
        let mut ws = Wanderers::default();
        let home = Pos { x: 500, y: 300 };
        let d = ws.tick("w", Some(MIN_IDLE), MIN_IDLE, home, 1440, 192, now);
        let Decision::Stroll { from, to } = d else { panic!("expected stroll, got {d:?}") };
        assert_eq!(from, home); assert_ne!(to, home); assert_eq!(to.y, home.y);
        ws.stroll_started("w");
        assert!(ws.is_strolling("w"));
        assert_eq!(ws.tick("w", Some(MIN_IDLE), MIN_IDLE, to, 1440, 192, now), Decision::Nothing, "no decisions mid-stroll");
        ws.stroll_finished("w", true, now);
        assert!(ws.ignores_reports("w", now), "settle window after a stroll");
        assert!(!ws.ignores_reports("w", now + SETTLE + Duration::from_millis(1)));
        // state changed to busy while away: go home immediately
        assert_eq!(ws.tick("w", None, MIN_IDLE, to, 1440, 192, now + Duration::from_secs(1)), Decision::GoHome { from: to, to: home });
        ws.stroll_started("w"); ws.stroll_finished("w", false, now);
        // user drags it while idle: new home, not away
        ws.rehome("w", Pos { x: 50, y: 60 }, now);
        assert_eq!(ws.get("w").unwrap().home, Pos { x: 50, y: 60 });
        assert!(!ws.get("w").unwrap().away);
    }
}

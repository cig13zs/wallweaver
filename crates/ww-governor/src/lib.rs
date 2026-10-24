//! The power governor decides, for each monitor, whether its wallpaper
//! runs at full rate, runs throttled, pauses, or stops and frees its GPU
//! memory.
//!
//! It is pure logic: no clocks, no OS calls. The daemon feeds it
//! [`Signals`] collected by the desktop host plus the current time (any
//! monotonic [`Duration`] since an arbitrary start), and it returns the
//! [`Transition`]s to apply. [`Governor::next_deadline`] says when to call
//! again if nothing else changes, so the daemon can sleep instead of
//! polling.
//!
//! Rules, from most to least restrictive:
//!
//! 1. An explicit "stop" from the user stops everything at once.
//! 2. Pause-all from the user, a locked session or sleeping displays pause.
//! 3. A fullscreen app pauses (by default on every monitor, not just its own).
//! 4. A monitor whose desktop is mostly covered pauses.
//! 5. Battery: below the low-battery threshold pauses; otherwise the
//!    battery action applies (throttle by default).
//! 6. Per-app rules for the foreground app.
//!
//! The most restrictive applicable rule wins. Restricting happens at once;
//! relaxing waits for [`Policy::resume_delay`] of stable signals, so
//! alt-tabbing through a game does not make the wallpaper flicker on and
//! off. A monitor that stays paused for [`Policy::stop_after`] is stopped so
//! its textures and pipelines are released.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Index of a monitor, as the desktop host numbers them.
pub type MonitorId = u32;

/// What a wallpaper should be doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum State {
    /// Drawing normally, up to `fps` frames a second.
    Running {
        /// Frame cap.
        fps: u32,
    },
    /// Drawing at a reduced rate.
    Throttled {
        /// Reduced frame cap.
        fps: u32,
    },
    /// Showing the last frame. No timers, no GPU work.
    Paused,
    /// GPU resources released. A still snapshot may be shown instead.
    Stopped,
}

impl State {
    /// Higher means more restrictive.
    pub fn severity(self) -> u8 {
        match self {
            State::Running { .. } => 0,
            State::Throttled { .. } => 1,
            State::Paused => 2,
            State::Stopped => 3,
        }
    }

    /// The frame cap, or 0 if the wallpaper must not draw.
    pub fn fps(self) -> u32 {
        match self {
            State::Running { fps } | State::Throttled { fps } => fps,
            State::Paused | State::Stopped => 0,
        }
    }

    /// True if the renderer may draw new frames.
    pub fn draws(self) -> bool {
        self.fps() > 0
    }
}

/// What to do in a given situation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Leave the wallpaper running.
    Run,
    /// Cap it at [`Policy::throttled_fps`].
    Throttle,
    /// Pause it.
    Pause,
    /// Stop it and free GPU memory.
    Stop,
}

/// Which monitors a fullscreen app pauses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    /// Only the monitor the app is on.
    SameMonitor,
    /// Every monitor.
    AllMonitors,
}

/// A rule for one application, matched case-insensitively against the
/// foreground app's executable name (for example `game.exe`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppRule {
    /// Executable name.
    pub app: String,
    /// What to do while it is in the foreground.
    pub action: Action,
}

/// The user's governor settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Policy {
    /// Frame cap while running.
    pub max_fps: u32,
    /// Frame cap while throttled.
    pub throttled_fps: u32,
    /// What a fullscreen app does.
    pub fullscreen: Action,
    /// Which monitors a fullscreen app affects.
    pub fullscreen_scope: Scope,
    /// What a mostly covered desktop does.
    pub covered: Action,
    /// Fraction of a monitor's desktop that counts as covered, 0.0 to 1.0.
    pub covered_threshold: f32,
    /// What running on battery does.
    pub on_battery: Action,
    /// Battery percentage at or below which wallpapers pause.
    pub low_battery_percent: u8,
    /// What the OS battery saver mode does.
    pub battery_saver: Action,
    /// Per-app rules.
    pub app_rules: Vec<AppRule>,
    /// How long signals must stay relaxed before a wallpaper resumes.
    #[serde(with = "millis")]
    pub resume_delay: Duration,
    /// How long a wallpaper stays paused before it is stopped. `None` keeps
    /// it paused forever.
    #[serde(with = "opt_millis")]
    pub stop_after: Option<Duration>,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            max_fps: 60,
            throttled_fps: 15,
            fullscreen: Action::Pause,
            fullscreen_scope: Scope::AllMonitors,
            covered: Action::Pause,
            covered_threshold: 0.95,
            on_battery: Action::Throttle,
            low_battery_percent: 20,
            battery_saver: Action::Pause,
            app_rules: Vec::new(),
            resume_delay: Duration::from_millis(500),
            stop_after: Some(Duration::from_secs(300)),
        }
    }
}

impl Policy {
    /// Clamp out-of-range values instead of failing: fps between 1 and 240,
    /// throttled no higher than max, threshold between 0.5 and 1.0, resume
    /// delay at most 10 s.
    pub fn sanitized(mut self) -> Policy {
        self.max_fps = self.max_fps.clamp(1, 240);
        self.throttled_fps = self.throttled_fps.clamp(1, self.max_fps);
        if !self.covered_threshold.is_finite() {
            self.covered_threshold = 0.95;
        }
        self.covered_threshold = self.covered_threshold.clamp(0.5, 1.0);
        self.low_battery_percent = self.low_battery_percent.min(100);
        self.resume_delay = self.resume_delay.min(Duration::from_secs(10));
        self
    }

    fn state_for(&self, action: Action) -> State {
        match action {
            Action::Run => State::Running { fps: self.max_fps },
            Action::Throttle => State::Throttled {
                fps: self.throttled_fps,
            },
            Action::Pause => State::Paused,
            Action::Stop => State::Stopped,
        }
    }
}

/// What one monitor's desktop host currently sees.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MonitorSignals {
    /// A fullscreen window (game, video player) is on this monitor.
    pub fullscreen_app: bool,
    /// Fraction of the desktop covered by other windows, 0.0 to 1.0.
    pub covered_fraction: f32,
    /// Executable name of the foreground app, if it is on this monitor.
    pub foreground_app: Option<String>,
}

/// Everything the governor looks at.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Signals {
    /// The user pressed pause-all.
    pub user_paused: bool,
    /// The user asked to stop all wallpapers.
    pub user_stopped: bool,
    /// The session is locked or the user switched away.
    pub session_locked: bool,
    /// All displays are asleep.
    pub displays_off: bool,
    /// Running on battery power.
    pub on_battery: bool,
    /// Battery charge, if known.
    pub battery_percent: Option<u8>,
    /// The OS battery saver mode is on.
    pub battery_saver: bool,
    /// Per-monitor signals. Monitors not listed have no wallpaper.
    pub monitors: BTreeMap<MonitorId, MonitorSignals>,
}

/// Why a monitor is in its current state.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "reason", content = "detail", rename_all = "camelCase")]
pub enum Reason {
    /// Nothing is holding it back.
    Normal,
    /// The user asked to stop.
    UserStopped,
    /// The user pressed pause-all.
    UserPaused,
    /// The session is locked.
    SessionLocked,
    /// The displays are asleep.
    DisplaysOff,
    /// A fullscreen app is running.
    FullscreenApp,
    /// The desktop is covered.
    Covered,
    /// Battery is low.
    LowBattery,
    /// Battery saver is on.
    BatterySaver,
    /// Running on battery.
    OnBattery,
    /// An app rule matched; the detail is the app name.
    AppRule(String),
    /// Paused long enough that resources were freed.
    PausedTooLong,
}

/// A change to apply to one monitor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    /// Which monitor.
    pub monitor: MonitorId,
    /// Previous state, or `None` for a monitor the governor just learned about.
    pub from: Option<State>,
    /// New state.
    pub to: State,
    /// Why.
    pub reason: Reason,
}

#[derive(Debug, Clone)]
struct Track {
    state: State,
    reason: Reason,
    /// When the state last changed.
    since: Duration,
    /// Last time signals asked for something at least as restrictive as the
    /// current state. Relaxing waits until `resume_delay` after this.
    last_restrictive: Duration,
}

/// The governor. One per daemon.
#[derive(Debug, Clone)]
pub struct Governor {
    policy: Policy,
    tracks: BTreeMap<MonitorId, Track>,
    last_signals: Signals,
}

impl Governor {
    /// A governor with the given policy and no monitors yet.
    pub fn new(policy: Policy) -> Governor {
        Governor {
            policy: policy.sanitized(),
            tracks: BTreeMap::new(),
            last_signals: Signals::default(),
        }
    }

    /// The active (sanitized) policy.
    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// Replace the policy. Takes effect on the next [`Governor::update`].
    pub fn set_policy(&mut self, policy: Policy) {
        self.policy = policy.sanitized();
    }

    /// Current state of one monitor.
    pub fn state(&self, monitor: MonitorId) -> Option<State> {
        self.tracks.get(&monitor).map(|t| t.state)
    }

    /// Current state and reason of every known monitor.
    pub fn states(&self) -> impl Iterator<Item = (MonitorId, State, &Reason)> {
        self.tracks.iter().map(|(m, t)| (*m, t.state, &t.reason))
    }

    /// The state the signals ask for right now, before any timing rules.
    pub fn desired(&self, signals: &Signals, monitor: MonitorId) -> (State, Reason) {
        let p = &self.policy;
        let mut best = (State::Running { fps: p.max_fps }, Reason::Normal);
        let mut consider = |action: Action, reason: Reason| {
            let s = p.state_for(action);
            if s.severity() > best.0.severity() {
                best = (s, reason);
            }
        };

        if signals.user_stopped {
            consider(Action::Stop, Reason::UserStopped);
        }
        if signals.user_paused {
            consider(Action::Pause, Reason::UserPaused);
        }
        if signals.session_locked {
            consider(Action::Pause, Reason::SessionLocked);
        }
        if signals.displays_off {
            consider(Action::Pause, Reason::DisplaysOff);
        }

        let mon = signals.monitors.get(&monitor);
        let fullscreen_here = mon.is_some_and(|m| m.fullscreen_app);
        let fullscreen_anywhere = signals.monitors.values().any(|m| m.fullscreen_app);
        let fullscreen = match p.fullscreen_scope {
            Scope::SameMonitor => fullscreen_here,
            Scope::AllMonitors => fullscreen_anywhere,
        };
        if fullscreen {
            consider(p.fullscreen, Reason::FullscreenApp);
        }
        if let Some(m) = mon
            && m.covered_fraction.is_finite()
            && m.covered_fraction >= p.covered_threshold
        {
            consider(p.covered, Reason::Covered);
        }

        if signals.on_battery {
            if signals
                .battery_percent
                .is_some_and(|b| b <= p.low_battery_percent)
            {
                consider(Action::Pause, Reason::LowBattery);
            }
            consider(p.on_battery, Reason::OnBattery);
        }
        if signals.battery_saver {
            consider(p.battery_saver, Reason::BatterySaver);
        }

        let foreground = signals
            .monitors
            .values()
            .filter_map(|m| m.foreground_app.as_deref());
        for app in foreground {
            for rule in &p.app_rules {
                if rule.app.eq_ignore_ascii_case(app) {
                    consider(rule.action, Reason::AppRule(rule.app.clone()));
                }
            }
        }
        best
    }

    /// Feed new signals at time `now` and get the changes to apply.
    ///
    /// Monitors missing from `signals.monitors` are forgotten (their
    /// wallpaper was removed or the display unplugged); the caller should
    /// release them, so no transition is reported for them.
    pub fn update(&mut self, now: Duration, signals: &Signals) -> Vec<Transition> {
        self.tracks.retain(|m, _| signals.monitors.contains_key(m));
        let mut out = Vec::new();
        let monitors: Vec<MonitorId> = signals.monitors.keys().copied().collect();
        for monitor in monitors {
            let (desired, reason) = self.desired(signals, monitor);
            let policy = self.policy.clone();
            let Some(track) = self.tracks.get_mut(&monitor) else {
                self.tracks.insert(
                    monitor,
                    Track {
                        state: desired,
                        reason: reason.clone(),
                        since: now,
                        last_restrictive: now,
                    },
                );
                out.push(Transition {
                    monitor,
                    from: None,
                    to: desired,
                    reason,
                });
                continue;
            };

            let current = track.state;
            let order = compare(desired, current, &track.reason);
            let next = match order {
                std::cmp::Ordering::Greater => {
                    // Restrict at once.
                    track.last_restrictive = now;
                    Some((desired, reason))
                }
                std::cmp::Ordering::Equal => {
                    track.last_restrictive = now;
                    let stays_stopped = current == State::Stopped && desired == State::Paused;
                    if !stays_stopped && (desired != current || reason != track.reason) {
                        // Same severity, different cap or reason (e.g. a new
                        // max_fps): apply directly.
                        Some((desired, reason))
                    } else {
                        None
                    }
                }
                std::cmp::Ordering::Less => {
                    // Relaxing. Paused → Stopped can still happen while we wait.
                    let relaxed_long_enough =
                        now.saturating_sub(track.last_restrictive) >= policy.resume_delay;
                    relaxed_long_enough.then_some((desired, reason))
                }
            };

            // Paused long enough (and staying paused): stop and free resources.
            let stays_paused = next.as_ref().is_none_or(|(to, _)| *to == State::Paused);
            let next = match (track.state, policy.stop_after) {
                (State::Paused, Some(after))
                    if stays_paused && now.saturating_sub(track.since) >= after =>
                {
                    Some((State::Stopped, Reason::PausedTooLong))
                }
                _ => next,
            };

            if let Some((to, reason)) = next
                && (to != track.state || reason != track.reason)
            {
                let from = track.state;
                if to != from {
                    track.since = now;
                }
                track.state = to;
                track.reason = reason.clone();
                if to != from {
                    out.push(Transition {
                        monitor,
                        from: Some(from),
                        to,
                        reason,
                    });
                }
            }
        }
        self.last_signals = signals.clone();
        out
    }

    /// When the governor next needs [`Governor::update`] called even if the
    /// signals don't change: a pending resume or a pause about to become a
    /// stop. `None` means it can wait for the next signal change.
    pub fn next_deadline(&self) -> Option<Duration> {
        let mut earliest: Option<Duration> = None;
        let mut take = |t: Duration| {
            earliest = Some(earliest.map_or(t, |e| e.min(t)));
        };
        for (monitor, track) in &self.tracks {
            let (desired, _) = self.desired(&self.last_signals, *monitor);
            if compare(desired, track.state, &track.reason) == std::cmp::Ordering::Less {
                take(track.last_restrictive + self.policy.resume_delay);
            }
            if track.state == State::Paused
                && let Some(after) = self.policy.stop_after
            {
                take(track.since + after);
            }
        }
        earliest
    }
}

/// Compare how restrictive `desired` is against the current state. A
/// monitor stopped because it was paused too long counts a paused request as
/// equal, so it stays stopped instead of waking up just to pause again.
fn compare(desired: State, current: State, reason: &Reason) -> std::cmp::Ordering {
    if current == State::Stopped && *reason == Reason::PausedTooLong && desired == State::Paused {
        return std::cmp::Ordering::Equal;
    }
    desired.severity().cmp(&current.severity())
}

mod millis {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(d.as_millis().min(u64::MAX as u128) as u64)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        Ok(Duration::from_millis(u64::deserialize(d)?))
    }
}

mod opt_millis {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(d: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(d) => s.serialize_some(&(d.as_millis().min(u64::MAX as u128) as u64)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        Ok(Option::<u64>::deserialize(d)?.map(Duration::from_millis))
    }
}

#[cfg(test)]
mod tests;

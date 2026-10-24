use std::time::Duration;

use proptest::prelude::*;

use super::*;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn two_monitors() -> Signals {
    let mut s = Signals::default();
    s.monitors.insert(0, MonitorSignals::default());
    s.monitors.insert(1, MonitorSignals::default());
    s
}

fn gov() -> Governor {
    Governor::new(Policy::default())
}

fn running() -> State {
    State::Running { fps: 60 }
}

#[test]
fn new_monitors_start_running() {
    let mut g = gov();
    let t = g.update(ms(0), &two_monitors());
    assert_eq!(t.len(), 2);
    assert!(t.iter().all(|t| t.from.is_none() && t.to == running()));
    assert_eq!(
        g.next_deadline(),
        None,
        "nothing pending, sleep until a signal"
    );
}

#[test]
fn fullscreen_pauses_every_monitor_by_default() {
    let mut g = gov();
    let mut s = two_monitors();
    g.update(ms(0), &s);
    s.monitors.get_mut(&1).unwrap().fullscreen_app = true;
    let t = g.update(ms(10), &s);
    assert_eq!(t.len(), 2);
    assert_eq!(g.state(0), Some(State::Paused));
    assert_eq!(g.state(1), Some(State::Paused));
    assert!(t.iter().all(|t| t.reason == Reason::FullscreenApp));
}

#[test]
fn fullscreen_same_monitor_scope() {
    let mut g = Governor::new(Policy {
        fullscreen_scope: Scope::SameMonitor,
        ..Policy::default()
    });
    let mut s = two_monitors();
    s.monitors.get_mut(&1).unwrap().fullscreen_app = true;
    g.update(ms(0), &s);
    assert_eq!(g.state(0), Some(running()));
    assert_eq!(g.state(1), Some(State::Paused));
}

#[test]
fn resume_waits_for_stable_signals() {
    let mut g = gov();
    let mut s = two_monitors();
    g.update(ms(0), &s);
    s.monitors.get_mut(&0).unwrap().fullscreen_app = true;
    g.update(ms(100), &s);
    s.monitors.get_mut(&0).unwrap().fullscreen_app = false;
    assert!(g.update(ms(200), &s).is_empty(), "too soon to resume");
    assert_eq!(g.next_deadline(), Some(ms(600)));
    assert!(g.update(ms(599), &s).is_empty());
    let t = g.update(ms(600), &s);
    assert_eq!(t.len(), 2);
    assert!(
        t.iter()
            .all(|t| t.to == running() && t.reason == Reason::Normal)
    );
}

#[test]
fn flapping_signals_keep_it_paused() {
    let mut g = gov();
    let mut s = two_monitors();
    g.update(ms(0), &s);
    // Alt-tab in and out of a game every 300 ms.
    for i in 1..20u64 {
        s.monitors.get_mut(&0).unwrap().fullscreen_app = i % 2 == 1;
        g.update(ms(i * 300), &s);
        assert_eq!(g.state(0), Some(State::Paused), "step {i}");
    }
}

#[test]
fn long_pause_becomes_stop_and_stays_stopped() {
    let mut g = Governor::new(Policy {
        stop_after: Some(Duration::from_secs(60)),
        ..Policy::default()
    });
    let mut s = two_monitors();
    s.user_paused = true;
    g.update(ms(0), &s);
    assert_eq!(g.state(0), Some(State::Paused));
    assert_eq!(g.next_deadline(), Some(Duration::from_secs(60)));
    let t = g.update(Duration::from_secs(60), &s);
    assert_eq!(t.len(), 2);
    assert!(
        t.iter()
            .all(|t| t.to == State::Stopped && t.reason == Reason::PausedTooLong)
    );
    // Still paused-worthy signals: stays stopped, nothing pending.
    assert!(g.update(Duration::from_secs(120), &s).is_empty());
    assert_eq!(g.state(0), Some(State::Stopped));
    assert_eq!(g.next_deadline(), None);
    // User resumes: back to running after the resume delay.
    s.user_paused = false;
    assert!(g.update(ms(120_200), &s).is_empty());
    let t = g.update(Duration::from_secs(122), &s);
    assert!(t.iter().all(|t| t.to == running()));
}

#[test]
fn no_stop_when_disabled() {
    let mut g = Governor::new(Policy {
        stop_after: None,
        ..Policy::default()
    });
    let mut s = two_monitors();
    s.session_locked = true;
    g.update(ms(0), &s);
    assert!(g.update(Duration::from_secs(100_000), &s).is_empty());
    assert_eq!(g.state(0), Some(State::Paused));
    assert_eq!(g.next_deadline(), None);
}

#[test]
fn user_stop_is_immediate() {
    let mut g = gov();
    let mut s = two_monitors();
    g.update(ms(0), &s);
    s.user_stopped = true;
    let t = g.update(ms(1), &s);
    assert!(
        t.iter()
            .all(|t| t.to == State::Stopped && t.reason == Reason::UserStopped)
    );
}

#[test]
fn battery_rules() {
    let mut g = gov();
    let mut s = two_monitors();
    s.on_battery = true;
    s.battery_percent = Some(80);
    g.update(ms(0), &s);
    assert_eq!(g.state(0), Some(State::Throttled { fps: 15 }));
    s.battery_percent = Some(20);
    let t = g.update(ms(10), &s);
    assert_eq!(t[0].reason, Reason::LowBattery);
    assert_eq!(g.state(0), Some(State::Paused));

    let mut g = Governor::new(Policy {
        on_battery: Action::Run,
        ..Policy::default()
    });
    s.battery_percent = Some(50);
    g.update(ms(0), &s);
    assert_eq!(g.state(0), Some(running()));
    s.battery_saver = true;
    g.update(ms(1), &s);
    assert_eq!(g.state(0), Some(State::Paused));
}

#[test]
fn covered_desktop_pauses_only_that_monitor() {
    let mut g = gov();
    let mut s = two_monitors();
    s.monitors.get_mut(&0).unwrap().covered_fraction = 0.97;
    s.monitors.get_mut(&1).unwrap().covered_fraction = 0.5;
    g.update(ms(0), &s);
    assert_eq!(g.state(0), Some(State::Paused));
    assert_eq!(g.state(1), Some(running()));
    s.monitors.get_mut(&1).unwrap().covered_fraction = f32::NAN;
    g.update(ms(1), &s);
    assert_eq!(g.state(1), Some(running()), "NaN is ignored");
}

#[test]
fn app_rules_match_case_insensitively() {
    let mut g = Governor::new(Policy {
        app_rules: vec![
            AppRule {
                app: "Editor.exe".into(),
                action: Action::Throttle,
            },
            AppRule {
                app: "render.exe".into(),
                action: Action::Stop,
            },
        ],
        ..Policy::default()
    });
    let mut s = two_monitors();
    s.monitors.get_mut(&0).unwrap().foreground_app = Some("editor.EXE".into());
    g.update(ms(0), &s);
    assert_eq!(g.state(1), Some(State::Throttled { fps: 15 }));
    s.monitors.get_mut(&0).unwrap().foreground_app = Some("RENDER.exe".into());
    let t = g.update(ms(1), &s);
    assert_eq!(t[0].reason, Reason::AppRule("render.exe".into()));
    assert_eq!(g.state(0), Some(State::Stopped));
}

#[test]
fn removed_monitors_are_forgotten() {
    let mut g = gov();
    let mut s = two_monitors();
    g.update(ms(0), &s);
    s.monitors.remove(&1);
    assert!(g.update(ms(1), &s).is_empty());
    assert_eq!(g.state(1), None);
    assert_eq!(g.states().count(), 1);
}

#[test]
fn policy_change_applies_new_fps() {
    let mut g = gov();
    let s = two_monitors();
    g.update(ms(0), &s);
    g.set_policy(Policy {
        max_fps: 30,
        ..Policy::default()
    });
    let t = g.update(ms(1), &s);
    assert_eq!(t.len(), 2);
    assert_eq!(g.state(0), Some(State::Running { fps: 30 }));
}

#[test]
fn policy_is_sanitized() {
    let p = Policy {
        max_fps: 0,
        throttled_fps: 1000,
        covered_threshold: f32::NAN,
        resume_delay: Duration::from_secs(3600),
        low_battery_percent: 250,
        ..Policy::default()
    }
    .sanitized();
    assert_eq!(p.max_fps, 1);
    assert_eq!(p.throttled_fps, 1);
    assert_eq!(p.covered_threshold, 0.95);
    assert_eq!(p.resume_delay, Duration::from_secs(10));
    assert_eq!(p.low_battery_percent, 100);
}

#[test]
fn policy_json_round_trip_and_defaults() {
    let p = Policy::default();
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains("\"resumeDelay\":500"), "{json}");
    assert!(json.contains("\"stopAfter\":300000"), "{json}");
    let back: Policy = serde_json::from_str(&json).unwrap();
    assert_eq!(back, p);
    let partial: Policy = serde_json::from_str(r#"{"maxFps": 30, "stopAfter": null}"#).unwrap();
    assert_eq!(partial.max_fps, 30);
    assert_eq!(partial.stop_after, None);
    assert_eq!(partial.throttled_fps, 15);
    assert!(serde_json::from_str::<Policy>(r#"{"turbo": true}"#).is_err());
}

#[test]
fn state_helpers() {
    assert_eq!(State::Paused.fps(), 0);
    assert!(!State::Stopped.draws());
    assert!(State::Throttled { fps: 5 }.draws());
    assert!(State::Stopped.severity() > State::Paused.severity());
    let json = serde_json::to_string(&State::Running { fps: 60 }).unwrap();
    assert_eq!(json, r#"{"state":"running","fps":60}"#);
}

// ---- property tests ----

fn arb_action() -> impl Strategy<Value = Action> {
    prop_oneof![
        Just(Action::Run),
        Just(Action::Throttle),
        Just(Action::Pause),
        Just(Action::Stop)
    ]
}

fn arb_policy() -> impl Strategy<Value = Policy> {
    (
        1u32..=240,
        1u32..=240,
        arb_action(),
        prop_oneof![Just(Scope::SameMonitor), Just(Scope::AllMonitors)],
        arb_action(),
        arb_action(),
        0u64..2000,
        proptest::option::of(0u64..5000),
    )
        .prop_map(|(max, thr, fs, scope, cov, bat, resume, stop)| Policy {
            max_fps: max,
            throttled_fps: thr,
            fullscreen: fs,
            fullscreen_scope: scope,
            covered: cov,
            on_battery: bat,
            resume_delay: ms(resume),
            stop_after: stop.map(ms),
            ..Policy::default()
        })
}

fn arb_monitor() -> impl Strategy<Value = MonitorSignals> {
    (any::<bool>(), 0.0f32..=1.0, any::<bool>()).prop_map(|(fs, cov, fg)| MonitorSignals {
        fullscreen_app: fs,
        covered_fraction: cov,
        foreground_app: fg.then(|| "app.exe".to_string()),
    })
}

fn arb_signals() -> impl Strategy<Value = Signals> {
    (
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        proptest::option::of(0u8..=100),
        proptest::collection::btree_map(0u32..3, arb_monitor(), 1..3),
    )
        .prop_map(
            |(paused, stopped, locked, battery, pct, monitors)| Signals {
                user_paused: paused,
                user_stopped: stopped,
                session_locked: locked,
                on_battery: battery,
                battery_percent: pct,
                monitors,
                ..Signals::default()
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Feed a random timeline and check the invariants after every step.
    #[test]
    fn invariants_hold_over_any_timeline(
        policy in arb_policy(),
        steps in proptest::collection::vec((0u64..1500, arb_signals()), 1..40),
    ) {
        let mut g = Governor::new(policy.clone());
        let p = g.policy().clone();
        let mut now = Duration::ZERO;
        for (dt, signals) in steps {
            now += ms(dt);
            let before: BTreeMap<_, _> = g.states().map(|(m, s, _)| (m, s)).collect();
            let transitions = g.update(now, &signals);

            for (m, state, _) in g.states() {
                let (desired, _) = g.desired(&signals, m);
                // Never less restrictive than the signals ask for, except a
                // Stopped monitor's "stays stopped" and pending relaxations
                // (which are always more restrictive).
                prop_assert!(state.severity() >= desired.severity(),
                    "monitor {m}: {state:?} is looser than desired {desired:?}");
                // Frame caps respect the policy.
                prop_assert!(state.fps() <= p.max_fps);
                if let State::Throttled { fps } = state {
                    prop_assert_eq!(fps, p.throttled_fps);
                }
                // Hard overrides.
                if signals.user_stopped {
                    prop_assert_eq!(state, State::Stopped);
                }
                if signals.user_paused || signals.session_locked {
                    prop_assert!(!state.draws());
                }
            }

            // Transitions describe exactly what changed.
            for t in &transitions {
                prop_assert_eq!(t.from, before.get(&t.monitor).copied());
                prop_assert_ne!(t.from, Some(t.to));
                prop_assert_eq!(g.state(t.monitor), Some(t.to));
            }
            for (m, s, _) in g.states() {
                if let Some(prev) = before.get(&m)
                    && *prev != s
                {
                    prop_assert!(transitions.iter().any(|t| t.monitor == m));
                }
            }

            // Deadlines are never in the past.
            if let Some(d) = g.next_deadline() {
                prop_assert!(d >= now, "deadline {d:?} before now {now:?}");
            }
        }
    }

    /// With no signal changes, calling update at each deadline converges:
    /// at most a resume and a stop per monitor, then no more deadlines
    /// (or only the stop timer of a paused monitor).
    #[test]
    fn deadlines_converge(policy in arb_policy(), first in arb_signals(), second in arb_signals()) {
        let mut g = Governor::new(policy);
        g.update(Duration::ZERO, &first);
        let mut now = ms(1);
        g.update(now, &second);
        let mut calls = 0;
        while let Some(d) = g.next_deadline() {
            now = now.max(d);
            g.update(now, &second);
            calls += 1;
            prop_assert!(calls <= 8, "deadlines did not converge");
        }
    }

    /// Relaxing never happens before the resume delay has passed since the
    /// signals last asked for the stricter state.
    #[test]
    fn resume_respects_delay(policy in arb_policy(), gap in 0u64..3000) {
        let mut g = Governor::new(Policy { stop_after: None, ..policy });
        let delay = g.policy().resume_delay;
        let mut s = Signals::default();
        s.monitors.insert(0, MonitorSignals::default());
        s.user_paused = true;
        g.update(Duration::ZERO, &s);
        s.user_paused = false;
        let t = g.update(ms(gap), &s);
        let resumed = g.state(0).is_some_and(|st| st.draws());
        let desired_draws = g.desired(&s, 0).0.draws();
        if desired_draws {
            prop_assert_eq!(resumed, ms(gap) >= delay, "gap {} delay {:?} {:?}", gap, delay, t);
        }
    }
}

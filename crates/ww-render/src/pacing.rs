//! When to draw the next frame. Pure logic so it can be tested without a GPU.
//!
//! The rule: draw only when something visible changes, never faster than the
//! frame cap, and never at all while paused. A still image draws once. A
//! slideshow draws when the image changes. An animated image draws when its
//! next frame is due. A shader changes every frame, so it draws at the cap.

use std::time::Duration;

/// When the content next changes, as reported by a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextChange {
    /// Never, unless something external (resize, new properties) happens.
    Never,
    /// At this time.
    At(Duration),
    /// Every frame (shaders).
    EveryFrame,
}

/// Pick the time of the next frame, or `None` to sleep until woken.
///
/// * `fps_cap` 0 means paused: no frames at all.
/// * `dirty` forces a frame (first frame, resize, new content), still
///   respecting the cap.
pub fn next_frame(
    change: NextChange,
    dirty: bool,
    last_frame: Option<Duration>,
    fps_cap: u32,
    now: Duration,
) -> Option<Duration> {
    if fps_cap == 0 {
        return None;
    }
    let earliest = match last_frame {
        Some(last) => last + frame_interval(fps_cap),
        None => now,
    };
    let wanted = if dirty {
        Some(now)
    } else {
        match change {
            NextChange::Never => None,
            NextChange::At(t) => Some(t),
            NextChange::EveryFrame => Some(now),
        }
    }?;
    Some(wanted.max(earliest))
}

/// Time between frames at a given cap.
pub fn frame_interval(fps_cap: u32) -> Duration {
    Duration::from_nanos(1_000_000_000 / u64::from(fps_cap.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulate a render loop for `secs` and count frames.
    fn simulate(secs: u64, fps_cap: u32, change: impl Fn(Duration) -> NextChange) -> u32 {
        let end = Duration::from_secs(secs);
        let mut now = Duration::ZERO;
        let mut last = None;
        let mut dirty = true;
        let mut frames = 0;
        while let Some(t) = next_frame(change(now), dirty, last, fps_cap, now) {
            if t >= end {
                break;
            }
            now = t;
            last = Some(now);
            dirty = false;
            frames += 1;
        }
        frames
    }

    #[test]
    fn still_image_draws_once() {
        assert_eq!(simulate(60, 60, |_| NextChange::Never), 1);
    }

    #[test]
    fn slideshow_draws_on_change_only() {
        let interval = Duration::from_secs(10);
        let frames = simulate(60, 60, |now| {
            NextChange::At(interval * (now.as_secs() / 10 + 1) as u32)
        });
        assert_eq!(frames, 6, "one frame per image over a minute");
    }

    #[test]
    fn animation_follows_frame_delays_up_to_the_cap() {
        let delay = Duration::from_millis(100);
        let next = |now: Duration| {
            let n = now.as_millis() as u32 / 100 + 1;
            NextChange::At(delay * n)
        };
        assert_eq!(simulate(10, 60, next), 100, "10 fps animation for 10 s");
        assert_eq!(simulate(10, 5, next), 50, "capped at 5 fps");
    }

    #[test]
    fn shader_runs_at_the_cap() {
        let frames = simulate(10, 30, |_| NextChange::EveryFrame);
        assert!((299..=301).contains(&frames), "{frames}");
    }

    #[test]
    fn paused_draws_nothing() {
        assert_eq!(simulate(10, 0, |_| NextChange::EveryFrame), 0);
        assert_eq!(
            next_frame(NextChange::EveryFrame, true, None, 0, Duration::ZERO),
            None
        );
    }

    #[test]
    fn dirty_respects_the_cap() {
        let last = Duration::from_millis(1000);
        let now = Duration::from_millis(1001);
        let t = next_frame(NextChange::Never, true, Some(last), 10, now).unwrap();
        assert_eq!(t, Duration::from_millis(1100));
    }
}

//! Rectangle math shared by every host.

/// A rectangle in desktop pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

impl Rect {
    /// Build a rectangle.
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// Build from edges; inverted edges give an empty rectangle.
    pub fn from_edges(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect {
            x: left,
            y: top,
            width: right.saturating_sub(left).max(0) as u32,
            height: bottom.saturating_sub(top).max(0) as u32,
        }
    }

    fn right(&self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    fn bottom(&self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }

    /// Area in pixels.
    pub fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// Overlapping area with `other`.
    pub fn overlap(&self, other: &Rect) -> u64 {
        let w = self.right().min(other.right()) - i64::from(self.x.max(other.x));
        let h = self.bottom().min(other.bottom()) - i64::from(self.y.max(other.y));
        if w <= 0 || h <= 0 {
            0
        } else {
            (w as u64) * (h as u64)
        }
    }

    /// True if this rectangle covers all of `other`, allowing `slack`
    /// pixels on each edge (window borders and DPI rounding).
    pub fn covers(&self, other: &Rect, slack: i32) -> bool {
        let s = i64::from(slack);
        i64::from(self.x) <= i64::from(other.x) + s
            && i64::from(self.y) <= i64::from(other.y) + s
            && self.right() >= other.right() - s
            && self.bottom() >= other.bottom() - s
    }

    /// Move into the coordinate space whose origin is at `origin`.
    pub fn relative_to(&self, origin: (i32, i32)) -> Rect {
        Rect {
            x: self.x - origin.0,
            y: self.y - origin.1,
            ..*self
        }
    }
}

/// Fraction (0.0 to 1.0) of `monitor` covered by `window`.
pub fn covered_fraction(window: &Rect, monitor: &Rect) -> f32 {
    let area = monitor.area();
    if area == 0 {
        return 0.0;
    }
    (window.overlap(monitor) as f64 / area as f64) as f32
}

/// The monitor that shows most of `window`, if any part of it is visible.
pub fn monitor_for<'a, I>(window: &Rect, monitors: I) -> Option<u32>
where
    I: IntoIterator<Item = (u32, &'a Rect)>,
{
    monitors
        .into_iter()
        .map(|(id, r)| (window.overlap(r), id))
        .filter(|(o, _)| *o > 0)
        .max_by_key(|(o, id)| (*o, std::cmp::Reverse(*id)))
        .map(|(_, id)| id)
}

/// The top-left corner of the box around all monitors, which is where
/// Windows' wallpaper layer has its origin.
pub fn virtual_origin<'a, I: IntoIterator<Item = &'a Rect>>(monitors: I) -> (i32, i32) {
    monitors
        .into_iter()
        .fold(None, |acc: Option<(i32, i32)>, r| {
            Some(match acc {
                None => (r.x, r.y),
                Some((x, y)) => (x.min(r.x), y.min(r.y)),
            })
        })
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: Rect = Rect::new(-1920, 0, 1920, 1080);
    const MAIN: Rect = Rect::new(0, 0, 2560, 1440);

    #[test]
    fn overlap_and_coverage() {
        let win = Rect::new(0, 0, 1280, 1440);
        assert_eq!(win.overlap(&MAIN), 1280 * 1440);
        assert_eq!(covered_fraction(&win, &MAIN), 0.5);
        assert_eq!(covered_fraction(&win, &LEFT), 0.0);
        assert_eq!(Rect::new(5000, 0, 10, 10).overlap(&MAIN), 0);
    }

    #[test]
    fn fullscreen_check_tolerates_borders() {
        let game = Rect::new(-1, -1, 2562, 1442);
        assert!(game.covers(&MAIN, 0));
        let almost = Rect::new(1, 0, 2559, 1440);
        assert!(!almost.covers(&MAIN, 0));
        assert!(almost.covers(&MAIN, 2));
    }

    #[test]
    fn window_belongs_to_the_monitor_showing_most_of_it() {
        let mons = [(0, &LEFT), (1, &MAIN)];
        assert_eq!(monitor_for(&Rect::new(-100, 0, 1000, 500), mons), Some(1));
        assert_eq!(monitor_for(&Rect::new(-900, 0, 1000, 500), mons), Some(0));
        assert_eq!(monitor_for(&Rect::new(9000, 0, 10, 10), mons), None);
    }

    #[test]
    fn virtual_origin_is_the_top_left_of_all_monitors() {
        assert_eq!(virtual_origin([&LEFT, &MAIN]), (-1920, 0));
        assert_eq!(MAIN.relative_to((-1920, 0)).x, 1920);
        assert_eq!(virtual_origin([]), (0, 0));
    }

    #[test]
    fn from_edges_handles_inverted_input() {
        assert_eq!(Rect::from_edges(10, 10, 5, 5).area(), 0);
        assert_eq!(Rect::from_edges(0, 0, 100, 50), Rect::new(0, 0, 100, 50));
    }
}

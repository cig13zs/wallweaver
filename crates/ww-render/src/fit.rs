//! Fitting content of one size onto a target of another.

use ww_manifest::Fit;

/// Where content lands on the target, in target pixels. May extend past the
/// target's edges (for `cover`), which the GPU clips.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Placement {
    /// The rectangle in normalized device coordinates: (left, top, right,
    /// bottom), with y pointing up.
    pub fn to_ndc(self, target: (u32, u32)) -> [f32; 4] {
        let (tw, th) = (target.0.max(1) as f32, target.1.max(1) as f32);
        let l = self.x / tw * 2.0 - 1.0;
        let r = (self.x + self.width) / tw * 2.0 - 1.0;
        let t = 1.0 - self.y / th * 2.0;
        let b = 1.0 - (self.y + self.height) / th * 2.0;
        [l, t, r, b]
    }

    /// True if the content covers every target pixel.
    pub fn covers(self, target: (u32, u32)) -> bool {
        self.x <= 0.0
            && self.y <= 0.0
            && self.x + self.width >= target.0 as f32
            && self.y + self.height >= target.1 as f32
    }
}

/// Place `content` on `target` according to `fit`, centered.
pub fn place(fit: Fit, content: (u32, u32), target: (u32, u32)) -> Placement {
    let (cw, ch) = (content.0.max(1) as f32, content.1.max(1) as f32);
    let (tw, th) = (target.0 as f32, target.1 as f32);
    let (w, h) = match fit {
        Fit::Stretch => (tw, th),
        Fit::Center => (cw, ch),
        Fit::Cover => {
            let s = (tw / cw).max(th / ch);
            (cw * s, ch * s)
        }
        Fit::Contain => {
            let s = (tw / cw).min(th / ch);
            (cw * s, ch * s)
        }
    };
    Placement {
        x: (tw - w) / 2.0,
        y: (th - h) / 2.0,
        width: w,
        height: h,
    }
}

/// The smallest size worth keeping for `content` shown on `target`: content
/// larger than it will appear is shrunk once on the CPU, so the GPU holds
/// no more pixels than the screen shows. Content is never enlarged here; the
/// GPU scales it up for free.
pub fn decode_size(fit: Fit, content: (u32, u32), target: (u32, u32)) -> (u32, u32) {
    let (cw, ch) = (content.0.max(1), content.1.max(1));
    match fit {
        Fit::Center => (cw, ch),
        Fit::Stretch => (cw.min(target.0.max(1)), ch.min(target.1.max(1))),
        Fit::Cover | Fit::Contain => {
            let p = place(fit, (cw, ch), target);
            let s = (p.width / cw as f32).min(1.0);
            if s >= 1.0 {
                (cw, ch)
            } else {
                (
                    ((cw as f32 * s - 0.01).ceil() as u32).clamp(1, cw),
                    ((ch as f32 * s - 0.01).ceil() as u32).clamp(1, ch),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_fills_and_crops() {
        let p = place(Fit::Cover, (1000, 1000), (1920, 1080));
        assert_eq!((p.width, p.height), (1920.0, 1920.0));
        assert_eq!(p.y, -420.0);
        assert!(p.covers((1920, 1080)));
    }

    #[test]
    fn contain_letterboxes() {
        let p = place(Fit::Contain, (1000, 500), (1000, 1000));
        assert_eq!((p.x, p.y, p.width, p.height), (0.0, 250.0, 1000.0, 500.0));
        assert!(!p.covers((1000, 1000)));
        assert_eq!(p.to_ndc((1000, 1000)), [-1.0, 0.5, 1.0, -0.5]);
    }

    #[test]
    fn stretch_and_center() {
        let p = place(Fit::Stretch, (10, 10), (30, 20));
        assert_eq!((p.x, p.y, p.width, p.height), (0.0, 0.0, 30.0, 20.0));
        let p = place(Fit::Center, (10, 10), (30, 20));
        assert_eq!((p.x, p.y, p.width, p.height), (10.0, 5.0, 10.0, 10.0));
    }

    #[test]
    fn decode_size_never_upscales_and_matches_screen() {
        // A 4K photo on a 1080p screen is kept at 1080p.
        assert_eq!(
            decode_size(Fit::Cover, (3840, 2160), (1920, 1080)),
            (1920, 1080)
        );
        // Portrait photo, cover: width decides.
        assert_eq!(
            decode_size(Fit::Cover, (3000, 4000), (1920, 1080)),
            (1920, 2560)
        );
        // Contain: height decides.
        assert_eq!(
            decode_size(Fit::Contain, (3000, 4000), (1920, 1080)),
            (810, 1080)
        );
        // Small images stay as they are.
        assert_eq!(
            decode_size(Fit::Cover, (640, 360), (1920, 1080)),
            (640, 360)
        );
        assert_eq!(
            decode_size(Fit::Stretch, (4000, 100), (1920, 1080)),
            (1920, 100)
        );
        assert_eq!(
            decode_size(Fit::Center, (4000, 4000), (1920, 1080)),
            (4000, 4000)
        );
    }
}

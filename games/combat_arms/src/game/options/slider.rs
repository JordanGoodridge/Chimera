//! The slider widget (specs/combat_arms/options.md W): every slider of
//! the options screen runs 0 to 100 over a track of `W` px. The thumb's
//! offset `d` runs 0 to `W`; the value is `floor(d * 100 / W)`. Showing a
//! stored value `v` puts the thumb at `floor((v + 1) * W / 100)`, so the
//! value read back can be `v + 1` (Quirk 2).
//!
//! The track width of a layout's slider is the position of its right
//! arrow minus the left arrow's, minus the left arrow's width and the
//! thumb's (`track_width`).

/// The range of every slider.
pub const RANGE: i32 = 100;

/// `max(1, floor(W / 100))`: the thumb's move per click on an arrow, px.
pub fn arrow_step(track: i32) -> i32 {
    (track / 100).max(1)
}

/// The track width from the layout's positions and widths, px.
pub fn track_width(left_arrow_x: i32, right_arrow_x: i32, left_arrow_w: i32, thumb_w: i32) -> i32 {
    right_arrow_x - left_arrow_x - left_arrow_w - thumb_w
}

/// A slider: its track and thumb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slider {
    /// The track's width `W`, px.
    pub track: i32,
    /// The thumb's offset `d`, px, 0 to `track`.
    pub thumb: i32,
}

impl Slider {
    pub fn new(track: i32) -> Self {
        Self { track: track.max(1), thumb: 0 }
    }

    /// The value the thumb reads, `floor(d * 100 / W)`.
    pub fn value(&self) -> i32 {
        self.thumb * RANGE / self.track
    }

    /// Show a stored value: the thumb goes to `floor((v + 1) * W / 100)`
    /// (capped at the track's end). Returns the value read back.
    pub fn show(&mut self, v: i32) -> i32 {
        self.thumb = ((v + 1) * self.track / RANGE).clamp(0, self.track);
        self.value()
    }

    /// A click on an arrow: `dir` -1 for the left one, 1 for the right.
    pub fn arrow(&mut self, dir: i32) -> i32 {
        self.thumb = (self.thumb + dir.signum() * arrow_step(self.track)).clamp(0, self.track);
        self.value()
    }

    /// A click on the track: the thumb's left edge goes to `x` (px from
    /// the track's start).
    pub fn click(&mut self, x: i32) -> i32 {
        self.thumb = x.clamp(0, self.track);
        self.value()
    }

    /// Dragging the thumb to `x`, clamped to the track.
    pub fn drag(&mut self, x: i32) -> i32 {
        self.click(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn showing_a_value_can_creep() {
        let mut s = Slider::new(100);
        assert_eq!(s.show(5), 6);
        assert_eq!(s.thumb, 6);
        let mut s = Slider::new(140);
        assert_eq!(s.show(5), 5);
        assert_eq!(s.thumb, 8);
    }
}

//! The radar's and the full-screen map's arithmetic (specs/combat_arms/
//! radar_minimap.md R1, R3 to R5, R7, R8, F3), as plain functions on file
//! coordinates and reference pixels (1024 x 768, y down), with no ECS and
//! no drawing. Section names are the spec's.
//!
//! Positions here are the spec's: world units, X east, Z north (the file's
//! axes), yaw psi from north turning right. Pixel results truncate toward
//! zero where the spec says so, in `f64` so a point off an integer
//! boundary never depends on float noise.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use bevy::math::{DVec2, Vec2};
use serde::{Deserialize, Serialize};

/// `DefaultWorldSize`: the radar window's world width at zoom 1.
pub const WORLD_SIZE: f64 = 1500.0;
/// The radar's clip window M: top-left and side (ref px).
pub const WINDOW_AT: (f64, f64) = (818.0, 5.0);
pub const WINDOW_SIDE: f64 = 186.0;
/// The radar's centre C (M's position + 93).
pub const CENTRE: (f64, f64) = (911.0, 98.0);
/// Ref px per world unit at zoom 1: 186 / 1500.
pub const SCALE_K: f64 = WINDOW_SIDE / WORLD_SIZE;
/// The map quad under the window: twice M, same centre (R5).
pub const QUAD_SIDE: f64 = 372.0;
/// The guide's centre, 2 and 3 px off C (quirk 4), and its side.
pub const GUIDE_CENTRE: (f64, f64) = (909.0, 95.0);
pub const GUIDE_SIDE: f64 = 512.0;
/// Every icon's box (ref px).
pub const ICON: (f64, f64) = (64.0, 32.0);
/// The edge arrow's inset from M's edge: (64 - 16) / 2.
pub const ARROW_INSET: f64 = 24.0;
/// Zoom range, default and the step per rendered frame (R3).
pub const ZOOM_MIN: f32 = 0.5;
pub const ZOOM_MAX: f32 = 2.0;
pub const ZOOM_DEFAULT: f32 = 1.0;
pub const ZOOM_STEP: f32 = 0.01;
/// The zoom bar knob: x, and y at zoom 2 (top) over a travel of 150.
pub const KNOB_X: i32 = 1004;
pub const KNOB_TOP: i32 = 18;
pub const KNOB_TRAVEL: f64 = 150.0;

/// The full map's image box and the share of the image cut from each side
/// (F2, F3).
pub const MAP_AT: (f64, f64) = (397.0, 139.0);
pub const MAP_SIDE: f64 = 512.0;
pub const MAP_CROP: f64 = 0.125;

/// The local player as the radar sees it: its file position and height,
/// its yaw (psi), the zoom and the rotate option.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub z: f64,
    pub y: f64,
    pub yaw: f64,
    pub zoom: f64,
    pub rotate: bool,
}

impl Frame {
    pub fn new(pos: (f64, f64), yaw: f64, zoom: f64, rotate: bool) -> Self {
        Self {
            x: pos.0,
            z: pos.1,
            y: 0.0,
            yaw,
            zoom,
            rotate,
        }
    }

    /// d' (R4): the offset of `v` from the player, turned by the yaw when
    /// the radar rotates.
    pub fn offset(&self, v: (f64, f64)) -> DVec2 {
        let d = DVec2::new(v.0 - self.x, v.1 - self.z);
        if self.rotate {
            let (s, c) = self.yaw.sin_cos();
            DVec2::new(d.x * c - d.y * s, d.x * s + d.y * c)
        } else {
            d
        }
    }

    /// The half side of the world window, 750 / z.
    pub fn half_range(&self) -> f64 {
        WORLD_SIZE / 2.0 / self.zoom
    }

    /// R4: `v` is in range when both |d'| are within 750 / z (a square;
    /// the edge counts).
    pub fn in_range(&self, v: (f64, f64)) -> bool {
        let d = self.offset(v);
        let r = self.half_range();
        d.x.abs() <= r && d.y.abs() <= r
    }

    /// R4: the icon's centre in ref px, None out of range.
    pub fn pixel(&self, v: (f64, f64)) -> Option<(i32, i32)> {
        if !self.in_range(v) {
            return None;
        }
        let d = self.offset(v);
        let r = self.half_range();
        let k = SCALE_K * self.zoom;
        let x = WINDOW_AT.0 + (k * (d.x + r)).trunc();
        let y = WINDOW_AT.1 + (-k * (d.y - r)).trunc();
        Some((x as i32, y as i32))
    }

    /// The angle the rotation takes out of facing objects: psi in the
    /// rotating mode, else 0 (R7's psi_rot).
    pub fn rot(&self) -> f64 {
        if self.rotate { self.yaw } else { 0.0 }
    }
}

/// R7: the four corners (top-left, top-right, bottom-right, bottom-left
/// of the texture) of a `w` x `h` quad centred on `centre`, turned to
/// `theta`, its size scaled by `s`, in screen px (reference px times
/// x_r, y_r).
pub fn quad_corners(centre: (f64, f64), w: f64, h: f64, theta: f64, s: f64, xr: f64, yr: f64) -> [Vec2; 4] {
    let a = (h / w).atan();
    let r = 0.5 * (w * w + h * h).sqrt();
    let c = [theta + a, theta + PI - a, theta + PI + a, theta + 2.0 * PI - a];
    c.map(|c| Vec2::new(((centre.0 + r * s * c.cos()) * xr) as f32, ((centre.1 + r * s * c.sin()) * yr) as f32))
}

/// R7: a character's facing wedge on the radar (`s` = the zoom) or the
/// full map (`s` = 1, `psi_rot` 0): 64 x 32 about (X, Y).
pub fn wedge(centre: (i32, i32), yaw_obj: f64, psi_rot: f64, s: f64, xr: f64, yr: f64) -> [Vec2; 4] {
    let theta = FRAC_PI_2 + yaw_obj - psi_rot;
    quad_corners((centre.0 as f64, centre.1 as f64), ICON.0, ICON.1, theta, s, xr, yr)
}

/// R7: the player's own wedge, the guide: upright and turning with the
/// view in fixed-north mode (angle pi + psi), not turned in rotating mode.
pub fn guide(frame: &Frame, xr: f64, yr: f64) -> [Vec2; 4] {
    let theta = if frame.rotate { PI } else { PI + frame.yaw };
    quad_corners(GUIDE_CENTRE, GUIDE_SIDE, GUIDE_SIDE, theta, 1.0, xr, yr)
}

/// R5: the map image's quad under the window: the layout rect fixed north,
/// turned about C by -psi in the rotating mode (angle pi - psi).
pub fn map_quad(frame: &Frame, xr: f64, yr: f64) -> [Vec2; 4] {
    let theta = if frame.rotate { PI - frame.yaw } else { PI };
    quad_corners(CENTRE, QUAD_SIDE, QUAD_SIDE, theta, 1.0, xr, yr)
}

/// The map image's corners in editor space (R1): the top-left and
/// bottom-right world points (X, Z).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Corners {
    pub lt: (f32, f32),
    pub rb: (f32, f32),
}

/// R1: the corners from two pixel to world pairs and the image size.
/// `f_x = (P1x - P0x) / (p1x - p0x)`, `f_y = (P0z - P1z) / (p1y - p0y)`;
/// a degenerate pair gives NaN, as the data's missing fields do (0 / 0).
pub fn corners(p0: (i32, i32), w0: (f64, f64), p1: (i32, i32), w1: (f64, f64), size: (i32, i32)) -> Corners {
    let fx = (w1.0 - w0.0) / (p1.0 - p0.0) as f64;
    let fy = (w0.1 - w1.1) / (p1.1 - p0.1) as f64;
    let lt = (w0.0 - p0.0 as f64 * fx, w0.1 + p0.1 as f64 * fy);
    let rb = (w1.0 + (size.0 - p1.0) as f64 * fx, w1.1 - (size.1 - p1.1) as f64 * fy);
    Corners {
        lt: (lt.0 as f32, lt.1 as f32),
        rb: (rb.0 as f32, rb.1 as f32),
    }
}

impl Corners {
    /// Both corners are real numbers (a map with data).
    pub fn is_finite(&self) -> bool {
        [self.lt.0, self.lt.1, self.rb.0, self.rb.1].iter().all(|v| v.is_finite())
    }
}

/// R5: the radar image's texture window, fractions of the image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub u0: f64,
    pub v0: f64,
    pub uw: f64,
    pub vh: f64,
}

/// R5: the window for a player at `frame`'s position, the world offset
/// `o` and the image's `corners`. (W_r = 1500 / z; the quad covers twice
/// that around the player.)
pub fn window(frame: &Frame, o: (f64, f64), c: &Corners) -> Window {
    let wr = WORLD_SIZE / frame.zoom;
    let ex = (c.lt.0 as f64 - c.rb.0 as f64).abs();
    let ez = (c.lt.1 as f64 - c.rb.1 as f64).abs();
    Window {
        u0: (frame.x - wr + o.0 - c.lt.0 as f64) / ex,
        v0: (c.lt.1 as f64 - (frame.z + wr + o.1)) / ez,
        uw: 2.0 * wr / ex,
        vh: 2.0 * wr / ez,
    }
}

/// R3: the zoom bar knob's y for zoom `z`.
pub fn knob_y(z: f32) -> i32 {
    // Rounded to 1e-4 first: the zoom is a sum of f32 steps.
    let z = (z as f64 * 1e4).round() / 1e4;
    KNOB_TOP + (KNOB_TRAVEL * (ZOOM_MAX as f64 - z) / 1.5).trunc() as i32
}

/// R3: one rendered frame of the zoom keys (expand makes it bigger;
/// both or neither change nothing), the result clamped to the range.
pub fn zoom_step(z: f32, expand: bool, decrease: bool) -> f32 {
    let d = match (expand, decrease) {
        (true, false) => ZOOM_STEP,
        (false, true) => -ZOOM_STEP,
        _ => 0.0,
    };
    (z + d).clamp(ZOOM_MIN, ZOOM_MAX)
}

/// R3: a saved zoom, restored clamped (default 1).
pub fn zoom_restore(z: f32) -> f32 {
    if z.is_finite() { z.clamp(ZOOM_MIN, ZOOM_MAX) } else { ZOOM_DEFAULT }
}

/// R8: an edge arrow: the point on the radar (ref px) and the angle beta
/// the arrow points along.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arrow {
    pub point: (i32, i32),
    pub beta: f64,
}

/// R8: the arrow for the direction (`dx`, `dy`, `dz`) from the player to
/// the object (rotated in the rotating mode; `dy` the height difference),
/// normalised in 3D.
pub fn arrow(dx: f64, dy: f64, dz: f64) -> Arrow {
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    let (nx, nz) = if len > 0.0 { (dx / len, dz / len) } else { (0.0, 0.0) };
    let beta = nx.atan2(nz);
    let b = beta.abs();
    let half = WINDOW_SIDE / 2.0;
    let l = if b > FRAC_PI_4 && b < 3.0 * FRAC_PI_4 {
        (half / b.sin()).abs() - ARROW_INSET
    } else {
        (half / b.cos()).abs() - ARROW_INSET
    }
    .trunc();
    Arrow {
        point: ((CENTRE.0 + (nx * l).trunc()) as i32, (CENTRE.1 + (-nz * l).trunc()) as i32),
        beta,
    }
}

/// R8: the arrow image's corners at `a`: 64 x 32, turned by pi / 2 + beta.
pub fn arrow_corners(a: &Arrow, xr: f64, yr: f64) -> [Vec2; 4] {
    quad_corners((a.point.0 as f64, a.point.1 as f64), ICON.0, ICON.1, FRAC_PI_2 + a.beta, 1.0, xr, yr)
}

/// F3: the full map's world to pixel transform for an image's corners and
/// the world offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FullMap {
    sx: f64,
    sy: f64,
    cx: f64,
    cy: f64,
    o: (f64, f64),
}

impl FullMap {
    pub fn new(c: &Corners, o: (f64, f64)) -> Self {
        let ex = (c.lt.0 as f64 - c.rb.0 as f64).abs();
        let ez = (c.lt.1 as f64 - c.rb.1 as f64).abs();
        let lt = (c.lt.0 as f64 + MAP_CROP * ex, c.lt.1 as f64 - MAP_CROP * ez);
        let rb = (c.rb.0 as f64 - MAP_CROP * ex, c.rb.1 as f64 + MAP_CROP * ez);
        let sx = MAP_SIDE / (rb.0 - lt.0);
        let sy = MAP_SIDE / (rb.1 - lt.1);
        // The centre offset is truncated on its own first (quirk 6).
        Self {
            sx,
            sy,
            cx: (-sx * lt.0).trunc(),
            cy: (-sy * lt.1).trunc(),
            o,
        }
    }

    /// F3: an object at file position `v`: the icon's centre (ref px).
    pub fn pixel(&self, v: (f64, f64)) -> (i32, i32) {
        let x = (MAP_AT.0 + self.cx + self.sx * (v.0 + self.o.0)).trunc();
        let y = (MAP_AT.1 + self.cy + self.sy * (v.1 + self.o.1)).trunc();
        (x as i32, y as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_is_a_square() {
        let f = Frame::new((0.0, 0.0), 0.0, 1.0, false);
        assert!(f.in_range((750.0, -750.0)));
        assert!(!f.in_range((750.1, 0.0)));
    }
}

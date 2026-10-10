//! Screen-space selection rectangle. No scene and no GPU.
//!
//! Left to right selects only what is fully enclosed. Right to left selects
//! anything the rectangle touches. A corner behind the camera is not inside.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarqueeKind {
    Window,
    Crossing,
}

/// Click versus drag. Four pixels is enough to ignore a shaky click.
pub const CLICK_SLOP_PX: f64 = 4.0;

#[derive(Clone, Copy, Debug)]
pub struct MarqueeDrag {
    pub origin_x: f64,
    pub origin_y: f64,
    pub x: f64,
    pub y: f64,
    pub shift: bool,
    pub ctrl: bool,
    pub moved: bool,
    /// Face, edge, or vertex box. An object marquee leaves this false.
    pub elements: bool,
}

impl MarqueeDrag {
    pub fn new(x: f64, y: f64, shift: bool, ctrl: bool) -> Self {
        Self { origin_x: x, origin_y: y, x, y, shift, ctrl, moved: false, elements: false }
    }

    pub fn update(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
        if (x - self.origin_x).hypot(y - self.origin_y) > CLICK_SLOP_PX {
            self.moved = true;
        }
    }
}

/// A few tenths of a pixel of leftward noise still counts as a window drag.
pub fn kind_for(x0: f64, x1: f64) -> MarqueeKind {
    if x1 + 0.5 >= x0 { MarqueeKind::Window } else { MarqueeKind::Crossing }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenRect {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl ScreenRect {
    pub fn from_drag(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { min_x: x0.min(x1), min_y: y0.min(y1), max_x: x0.max(x1), max_y: y0.max(y1) }
    }

    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    pub fn overlaps(self, min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> bool {
        self.min_x <= max_x && self.max_x >= min_x && self.min_y <= max_y && self.max_y >= min_y
    }
}

/// One box corner in viewport pixels. `in_front` is false when the camera cannot see it.
#[derive(Clone, Copy, Debug)]
pub struct ProjectedCorner {
    pub x: f64,
    pub y: f64,
    pub in_front: bool,
}

/// `corners` is the eight corners of one object. Window selection fails unless every
/// corner is in front of the camera and inside the rectangle.
pub fn selection_hits(kind: MarqueeKind, rect: ScreenRect, corners: &[ProjectedCorner]) -> bool {
    match kind {
        MarqueeKind::Window => {
            corners.len() == 8 && corners.iter().all(|corner| corner.in_front && rect.contains(corner.x, corner.y))
        }
        MarqueeKind::Crossing => {
            let front: Vec<_> = corners.iter().copied().filter(|corner| corner.in_front).collect();
            if front.is_empty() {
                return false;
            }
            let min_x = front.iter().map(|corner| corner.x).fold(f64::INFINITY, f64::min);
            let min_y = front.iter().map(|corner| corner.y).fold(f64::INFINITY, f64::min);
            let max_x = front.iter().map(|corner| corner.x).fold(f64::NEG_INFINITY, f64::max);
            let max_y = front.iter().map(|corner| corner.y).fold(f64::NEG_INFINITY, f64::max);
            rect.overlaps(min_x, min_y, max_x, max_y)
        }
    }
}

/// Authored element points. A vertex is one point, an edge is its endpoints, and a face is its vertex loop.
///
/// Window selection needs every point in front and inside. Crossing selection uses the front bounds.
/// The point count is the element's, not the eight corners of an object box.
pub fn element_hits(kind: MarqueeKind, rect: ScreenRect, points: &[ProjectedCorner]) -> bool {
    if points.is_empty() {
        return false;
    }
    match kind {
        MarqueeKind::Window => points.iter().all(|point| point.in_front && rect.contains(point.x, point.y)),
        MarqueeKind::Crossing => {
            let front: Vec<_> = points.iter().copied().filter(|point| point.in_front).collect();
            if front.is_empty() {
                return false;
            }
            let min_x = front.iter().map(|point| point.x).fold(f64::INFINITY, f64::min);
            let min_y = front.iter().map(|point| point.y).fold(f64::INFINITY, f64::min);
            let max_x = front.iter().map(|point| point.x).fold(f64::NEG_INFINITY, f64::max);
            let max_y = front.iter().map(|point| point.y).fold(f64::NEG_INFINITY, f64::max);
            rect.overlaps(min_x, min_y, max_x, max_y)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corner(x: f64, y: f64, in_front: bool) -> ProjectedCorner {
        ProjectedCorner { x, y, in_front }
    }

    fn cube() -> Vec<ProjectedCorner> {
        vec![
            corner(10.0, 10.0, true),
            corner(40.0, 10.0, true),
            corner(10.0, 40.0, true),
            corner(40.0, 40.0, true),
            corner(12.0, 12.0, true),
            corner(38.0, 12.0, true),
            corner(12.0, 38.0, true),
            corner(38.0, 38.0, true),
        ]
    }

    #[test]
    fn window_requires_every_corner_and_crossing_uses_the_touch() {
        assert_eq!(kind_for(10.0, 80.0), MarqueeKind::Window);
        assert_eq!(kind_for(80.0, 10.0), MarqueeKind::Crossing);
        assert_eq!(kind_for(10.0, 9.7), MarqueeKind::Window);
        let enclosed = ScreenRect::from_drag(0.0, 0.0, 50.0, 50.0);
        assert!(selection_hits(MarqueeKind::Window, enclosed, &cube()));
        let tight = ScreenRect::from_drag(20.0, 20.0, 30.0, 30.0);
        assert!(!selection_hits(MarqueeKind::Window, tight, &cube()));
        assert!(selection_hits(MarqueeKind::Crossing, tight, &cube()));
        let miss = ScreenRect::from_drag(100.0, 100.0, 120.0, 110.0);
        assert!(!selection_hits(MarqueeKind::Crossing, miss, &cube()));
        let mut behind = cube();
        behind[0].in_front = false;
        assert!(!selection_hits(MarqueeKind::Window, enclosed, &behind));
        let all_behind: Vec<_> = cube().into_iter().map(|mut corner| { corner.in_front = false; corner }).collect();
        assert!(!selection_hits(MarqueeKind::Crossing, enclosed, &all_behind));
        assert!(!selection_hits(MarqueeKind::Window, enclosed, &cube()[..7]));
        let face = vec![corner(0.0, 0.0, true), corner(10.0, 0.0, true), corner(0.0, 10.0, true)];
        let tight = ScreenRect::from_drag(0.0, 0.0, 10.0, 10.0);
        assert!(element_hits(MarqueeKind::Window, tight, &face));
        assert!(element_hits(MarqueeKind::Window, tight, &face[..1]));
        assert!(!element_hits(MarqueeKind::Window, ScreenRect::from_drag(1.0, 1.0, 4.0, 4.0), &face));
        assert!(element_hits(MarqueeKind::Crossing, ScreenRect::from_drag(8.0, 8.0, 12.0, 12.0), &face));
        let mut behind = face.clone();
        behind[0].in_front = false;
        assert!(!element_hits(MarqueeKind::Window, tight, &behind));
        assert!(!element_hits(MarqueeKind::Window, tight, &[]));
        assert!(!element_hits(MarqueeKind::Crossing, tight, &[]));
    }
}

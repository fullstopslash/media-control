//! Geometry primitives shared between window-management commands.
//!
//! `Rect` collapses the 8-arg overlap signature that previously sprawled
//! across `avoid.rs` and is the home for any future axis-aligned geometry
//! helpers.

use super::{
    CommandContext, resolve_effective_position_with_minified, resolve_position_or_with_minified,
};
use crate::hyprland::Client;
use crate::window::MediaWindow;

/// Axis-aligned rectangle in Hyprland window coordinates.
///
/// All edge arithmetic in [`Rect::overlaps`] is performed in `i64` to defend
/// against pathological geometry coming from the Hyprland socket — adding
/// two `i32`s near the limits would overflow and silently flip the
/// comparison result. Tests at avoid.rs (overflow / extreme-edge cases)
/// rely on this widening; do not weaken it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    /// Construct from the `(x, y, w, h)` tuple shape used by call sites that
    /// have loose `i32` locals rather than a struct.
    #[inline]
    pub(crate) const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    /// Construct from a [`Client`]'s `at` / `size` arrays.
    #[inline]
    pub(crate) fn from_client(c: &Client) -> Self {
        Self::new(c.at[0], c.at[1], c.size[0], c.size[1])
    }

    /// Construct from a [`MediaWindow`]'s positional fields.
    #[inline]
    pub(crate) fn from_media(w: &MediaWindow) -> Self {
        Self::new(w.x, w.y, w.width, w.height)
    }

    /// True iff `self` and `other` share at least one interior pixel.
    ///
    /// Degenerate rectangles (zero or negative width/height) never overlap.
    /// All edge arithmetic widens to `i64` so adversarial socket payloads
    /// near `i32::MAX`/`i32::MIN` cannot wrap and flip the comparison.
    #[inline]
    pub(crate) fn overlaps(&self, other: &Rect) -> bool {
        if self.w <= 0 || self.h <= 0 || other.w <= 0 || other.h <= 0 {
            return false;
        }
        let (ax, ay, bx, by) = (
            i64::from(self.x),
            i64::from(self.y),
            i64::from(other.x),
            i64::from(other.y),
        );
        let (aw, ah, bw, bh) = (
            i64::from(self.w),
            i64::from(self.h),
            i64::from(other.w),
            i64::from(other.h),
        );
        !(ax >= bx + bw || bx >= ax + aw || ay >= by + bh || by >= ay + ah)
    }
}

/// Single-shot position resolver that captures the `minified` flag once so
/// callers stop rebuilding their per-call resolve closure (and stop
/// re-stat'ing the minify marker file four times in a row).
///
/// The `minified` flag is computed by the caller via [`super::is_minified`]
/// exactly once per `avoid()` tick; passing the bool through avoids the
/// redundant filesystem stats that previously showed up in flame graphs.
pub(crate) struct PositionResolver<'a> {
    pub ctx: &'a CommandContext,
    pub minified: bool,
}

impl<'a> PositionResolver<'a> {
    #[inline]
    pub(crate) fn new(ctx: &'a CommandContext, minified: bool) -> Self {
        Self { ctx, minified }
    }

    /// Resolve `name`, falling back to `default` when it isn't a known
    /// position label.
    #[inline]
    pub(crate) fn resolve_or(&self, name: &str, default: i32) -> i32 {
        resolve_position_or_with_minified(self.ctx, name, default, self.minified)
    }

    /// Resolve `name`, returning `None` for unknown labels.
    #[inline]
    pub(crate) fn resolve_opt(&self, name: &str) -> Option<i32> {
        resolve_effective_position_with_minified(self.ctx, name, self.minified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlaps_basic() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(50, 50, 100, 100);
        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));
    }

    #[test]
    fn overlaps_touching_edges() {
        // Touching but not overlapping → false.
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(100, 0, 100, 100);
        assert!(!a.overlaps(&b));
    }

    #[test]
    fn overlaps_contained() {
        let outer = Rect::new(0, 0, 1000, 1000);
        let inner = Rect::new(100, 100, 50, 50);
        assert!(outer.overlaps(&inner));
        assert!(inner.overlaps(&outer));
    }

    #[test]
    fn overlaps_degenerate_dimensions() {
        let a = Rect::new(0, 0, 100, 100);
        assert!(!a.overlaps(&Rect::new(50, 50, 0, 50)));
        assert!(!a.overlaps(&Rect::new(50, 50, -10, 50)));
        assert!(!Rect::new(0, 0, 0, 100).overlaps(&a));
    }

    #[test]
    fn overlaps_no_overflow_at_extremes() {
        // Pre-i64-widening, x2 + w2 would wrap and silently flip the result.
        let a = Rect::new(i32::MAX - 200, 0, 100, 100);
        let b = Rect::new(i32::MAX - 100, 0, 100, 100);
        assert!(!a.overlaps(&b));
    }
}

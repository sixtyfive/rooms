//! Pure geometry for the tray popup (physical pixels). No Tauri types, so it is unit-testable.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Bottom,
    Top,
    Left,
    Right,
}

/// The taskbar sits on the monitor edge nearest to the tray icon (ties prefer the bottom).
pub fn nearest_edge(anchor: Rect, monitor: Rect) -> Edge {
    let (cx, cy) = anchor.center();
    [
        (Edge::Bottom, monitor.bottom() - cy),
        (Edge::Top, cy - monitor.y),
        (Edge::Left, cx - monitor.x),
        (Edge::Right, monitor.right() - cx),
    ]
    .into_iter()
    .min_by_key(|&(_, d)| d)
    .map(|(e, _)| e)
    .unwrap()
}

fn clamp(v: i32, lo: i32, hi: i32) -> i32 {
    v.max(lo).min(hi.max(lo))
}

/// Docks a `want_w` x `want_h` popup against the taskbar like the native flyouts do:
/// flush with the work area on the taskbar's side, centred on the icon along the other axis,
/// then clamped into the work area. Oversized popups are shrunk to fit.
pub fn place(anchor: Rect, work: Rect, monitor: Rect, want_w: i32, want_h: i32, gap: i32) -> Rect {
    let w = want_w.min(work.w - 2 * gap).max(1);
    let h = want_h.min(work.h - 2 * gap).max(1);
    let (cx, cy) = anchor.center();
    let (x, y) = match nearest_edge(anchor, monitor) {
        Edge::Bottom => (cx - w / 2, work.bottom() - h - gap),
        Edge::Top => (cx - w / 2, work.y + gap),
        Edge::Left => (work.x + gap, cy - h / 2),
        Edge::Right => (work.right() - w - gap, cy - h / 2),
    };
    Rect {
        x: clamp(x, work.x + gap, work.right() - w - gap),
        y: clamp(y, work.y + gap, work.bottom() - h - gap),
        w,
        h,
    }
}

/// Anchor used when no tray rectangle is known: the bottom-right corner of the work area.
pub fn corner_anchor(work: Rect, monitor: Rect) -> Rect {
    Rect { x: work.right() - 1, y: monitor.bottom() - 1, w: 1, h: 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Rect = Rect { x: 0, y: 0, w: 1920, h: 1080 };
    const WORK: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 }; // 40 px taskbar at the bottom

    fn icon(x: i32, y: i32) -> Rect {
        Rect { x, y, w: 24, h: 40 }
    }

    #[test]
    fn bottom_taskbar_centres_on_icon_and_sits_on_work_area() {
        let r = place(icon(1000, 1040), WORK, MON, 960, 400, 8);
        assert_eq!(r, Rect { x: 1012 - 480, y: 1040 - 400 - 8, w: 960, h: 400 });
    }

    #[test]
    fn clamps_at_right_edge() {
        let r = place(icon(1890, 1040), WORK, MON, 960, 400, 8);
        assert_eq!(r.x, 1920 - 960 - 8);
        assert_eq!(r.right() + 8, 1920);
    }

    #[test]
    fn clamps_at_left_edge() {
        let r = place(icon(10, 1040), WORK, MON, 960, 400, 8);
        assert_eq!(r.x, 8);
    }

    #[test]
    fn overflow_flyout_icon_still_docks_to_taskbar() {
        // icon inside the hidden-icons flyout, well above the taskbar
        let r = place(Rect { x: 1700, y: 900, w: 24, h: 24 }, WORK, MON, 960, 400, 8);
        assert_eq!(r.bottom(), WORK.bottom() - 8);
    }

    #[test]
    fn top_taskbar_opens_downwards() {
        let work = Rect { x: 0, y: 40, w: 1920, h: 1040 };
        let r = place(Rect { x: 900, y: 0, w: 24, h: 40 }, work, MON, 960, 400, 8);
        assert_eq!(r.y, 48);
    }

    #[test]
    fn left_and_right_taskbars() {
        let work = Rect { x: 60, y: 0, w: 1860, h: 1080 };
        let r = place(Rect { x: 0, y: 500, w: 60, h: 24 }, work, MON, 960, 400, 8);
        assert_eq!((r.x, r.y), (68, 512 - 200));
        let work = Rect { x: 0, y: 0, w: 1860, h: 1080 };
        let r = place(Rect { x: 1860, y: 500, w: 60, h: 24 }, work, MON, 960, 400, 8);
        assert_eq!(r.right() + 8, 1860);
    }

    #[test]
    fn oversized_popup_shrinks_to_work_area() {
        let r = place(icon(1000, 1040), WORK, MON, 3000, 5000, 8);
        assert_eq!((r.w, r.h), (1920 - 16, 1040 - 16));
        assert_eq!((r.x, r.y), (8, 8));
    }

    #[test]
    fn secondary_monitor_with_offset() {
        let mon = Rect { x: 1920, y: 0, w: 2560, h: 1440 };
        let work = Rect { x: 1920, y: 0, w: 2560, h: 1400 };
        let r = place(Rect { x: 4400, y: 1400, w: 30, h: 40 }, work, mon, 1200, 600, 12);
        assert_eq!(r.right() + 12, 4480);
        assert_eq!(r.bottom() + 12, 1400);
    }

    #[test]
    fn corner_fallback_lands_bottom_right() {
        let r = place(corner_anchor(WORK, MON), WORK, MON, 960, 400, 8);
        assert_eq!(r.right() + 8, 1920);
        assert_eq!(r.bottom() + 8, 1040);
    }
}

//! Timeline: transport strip with the timecode, a ruler with major and minor
//! ticks, one track per layer aligned with the names column, lifetime bars
//! with keyframe diamonds, and the playhead.

use crate::icons::{self, Icon};
use crate::layout::{Hit, Rect, TransportButton};
use crate::outline::OutlineRow;
use crate::paint::Painter;
use crate::panels::View;
use crate::text::Align;
use crate::theme::{size, type_scale};
use crate::timeline::{frame_to_x, tick_frames, tick_spacing, TickUnit, MIN_TICK_SPACING_PX};
use crate::transport::timecode;
use crate::widgets::{self, ButtonKind};

/// Draws the timeline.
pub fn draw(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    p.fill(tl.rect, t.panel);
    p.hline(tl.rect.x, tl.rect.right(), tl.rect.y, t.hairline);

    header(p, v);
    ruler(p, v);
    tracks(p, v);
    if v.document.is_some() {
        playhead(p, v);
    }
}

fn header(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    let header = tl.header;
    p.hline(
        header.x,
        header.right(),
        header.bottom() - p.m.hairline(),
        t.hairline,
    );

    let pad = p.m.px(size::PAD);
    let loaded = v.document.is_some();
    let tc_style = p
        .style(
            type_scale::TITLE,
            if loaded { t.text } else { t.text_disabled },
        )
        .medium()
        .tracked(0.02);
    let tc = if loaded {
        timecode(v.frame, v.rate)
    } else {
        "--:--:--:--".to_owned()
    };
    let tc_w = p.measure(&tc, &tc_style);
    let tc_rect = Rect::new(header.x + pad, header.y, tc_w + 1.0, header.h);
    p.text_in(&tc, tc_rect, Align::Left, &tc_style);
    let counter_style = p.style(type_scale::CAPTION, t.text_muted);
    let counter = if loaded {
        format!("{} / {}", v.frame, v.duration.max(1) - 1)
    } else {
        String::new()
    };
    let counter_rect = Rect::new(
        tc_rect.right() + p.m.px(size::GRID * 3.0),
        header.y,
        p.m.px(96.0),
        header.h,
    );
    p.text_in(&counter, counter_rect, Align::Left, &counter_style);

    for (button, rect) in v.layout.transport {
        let (icon, kind) = match button {
            TransportButton::ToStart => (Icon::ToStart, ButtonKind::Ghost),
            TransportButton::StepBack => (Icon::StepBack, ButtonKind::Ghost),
            TransportButton::PlayPause => (
                if v.playing { Icon::Pause } else { Icon::Play },
                ButtonKind::Primary,
            ),
            TransportButton::StepForward => (Icon::StepForward, ButtonKind::Ghost),
            TransportButton::ToEnd => (Icon::ToEnd, ButtonKind::Ghost),
            TransportButton::Loop => (Icon::Loop, ButtonKind::Ghost),
        };
        let mut state = v.interaction(Hit::Transport(button));
        state.active = button == TransportButton::Loop && v.looping;
        state.disabled = v.document.is_none();
        widgets::icon_button(p, rect, icon, kind, state);
    }

    let rate = v.rate;
    let seconds = v.duration as f64 * f64::from(rate.denominator()) / f64::from(rate.numerator());
    let info = format!("{seconds:.2} s  ·  {} frames", v.duration);
    let info_w = p.measure(&info, &counter_style);
    let info_rect = Rect::new(
        header.right() - pad - info_w,
        header.y,
        info_w + 1.0,
        header.h,
    );
    let transport_end = v.layout.transport[5].1.right();
    if loaded && info_rect.x > transport_end + pad {
        p.text_in(&info, info_rect, Align::Left, &counter_style);
    }
}

fn ruler(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    p.fill(tl.corner, t.raised);
    p.fill(tl.ruler, t.raised);
    p.hline(
        tl.ruler.x - tl.corner.w,
        tl.ruler.right(),
        tl.ruler.bottom() - p.m.hairline(),
        t.hairline,
    );
    let caption = widgets::caption(p, t.text_disabled);
    p.text_in(
        "LAYER",
        tl.corner.inset_xy(p.m.px(size::PAD), 0.0),
        Align::Left,
        &caption,
    );
    if v.document.is_none() {
        return;
    }

    let scrub = tl.scrub;
    let duration = v.duration.max(1);
    let spacing = tick_spacing(
        duration,
        v.rate,
        f64::from(scrub.w),
        MIN_TICK_SPACING_PX * f64::from(p.m.scale),
    );
    let head = v.document.is_some().then(|| head_rect(p, v).0);
    p.push_clip(tl.ruler);
    let label_style = p.style(type_scale::CAPTION, t.text_muted);
    let major_h = p.m.px(8.0);
    let minor_h = p.m.px(4.0);
    let bottom = tl.ruler.bottom() - p.m.hairline();
    let step = spacing.step_frames.max(1);
    let minor_step = minor_step(step);
    let mut frame = 0;
    while frame < duration {
        let x = frame_to_x(frame, duration, f64::from(scrub.x), f64::from(scrub.w)).round() as f32;
        if frame % step == 0 {
            p.vline(x, bottom - major_h, bottom, t.text_disabled);
        } else {
            p.vline(x, bottom - minor_h, bottom, t.border);
        }
        frame += minor_step;
    }
    for tick in tick_frames(duration, spacing) {
        let x = frame_to_x(tick, duration, f64::from(scrub.x), f64::from(scrub.w)).round() as f32;
        let label = match spacing.unit {
            TickUnit::Frames => tick.to_string(),
            TickUnit::Seconds => {
                let seconds =
                    tick as f64 * f64::from(v.rate.denominator()) / f64::from(v.rate.numerator());
                format!("{seconds:.0}s")
            }
        };
        let rect = Rect::new(
            x + p.m.px(size::GRID),
            tl.ruler.y,
            p.m.px(48.0),
            tl.ruler.h - major_h,
        );
        // Labels under the playhead tab would peek out beside it.
        let label_rect = Rect::new(rect.x, rect.y, p.measure(&label, &label_style), rect.h);
        if head.is_some_and(|h| h.overlaps(label_rect)) {
            continue;
        }
        p.text_in(&label, rect, Align::Left, &label_style);
    }
    // Last frame marker so the end of the composition is explicit.
    let end = frame_to_x(
        duration - 1,
        duration,
        f64::from(scrub.x),
        f64::from(scrub.w),
    )
    .round() as f32;
    p.vline(end, bottom - major_h, bottom, t.text_disabled);
    p.pop_clip();
}

/// Minor tick step: a fifth or a sixth of the major step when that divides
/// it, otherwise every frame for tiny steps.
fn minor_step(major: i64) -> i64 {
    for parts in [6, 5, 4, 2] {
        if major % parts == 0 && major / parts >= 1 {
            return major / parts;
        }
    }
    major.max(1)
}

fn tracks(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    p.fill(tl.tracks, t.app);
    p.vline(
        tl.names.right() - p.m.hairline(),
        tl.corner.y,
        tl.names.bottom(),
        t.hairline,
    );

    let area = Rect::new(tl.names.x, tl.names.y, tl.rect.w, tl.names.h);
    p.push_clip(area);
    for (index, row) in v.outline.iter().enumerate() {
        let rect = v.layout.track_row(index, v.track_scroll);
        if rect.bottom() >= area.y && rect.y <= area.bottom() {
            track_row(p, v, index, row, rect);
        }
    }
    if v.outline.is_empty() {
        let style = p.style(type_scale::CAPTION, t.text_disabled);
        let pad = p.m.px(size::PAD);
        let rect = Rect::new(
            tl.names.x + pad,
            tl.names.y,
            tl.names.w - pad,
            p.m.px(size::ROW),
        );
        p.text_in("No tracks", rect, Align::Left, &style);
    }
    p.pop_clip();
}

/// One track: highlight, name in the names column, lifetime bar, animated
/// span and keyframe diamonds.
fn track_row(p: &mut Painter<'_>, v: &View<'_>, index: usize, row: &OutlineRow, rect: Rect) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    let duration = v.duration.max(1);
    let scrub = tl.scrub;
    let x_of =
        |frame: i64| frame_to_x(frame, duration, f64::from(scrub.x), f64::from(scrub.w)) as f32;
    let pad = p.m.px(size::PAD);
    let icon_side = p.m.px(14.0);
    let bar_h = p.m.px(12.0);
    let diamond = p.m.px(10.0);
    let selected = v.selected == Some(index);
    if selected {
        p.fill(rect, t.selection);
    } else if v.hovered_row() == Some(index) {
        p.fill(rect, t.row_hover);
    }
    p.hline(
        rect.x,
        rect.right(),
        rect.bottom() - p.m.hairline(),
        t.hairline,
    );

    // Names column.
    let x = tl.names.x + pad + row.depth as f32 * p.m.px(12.0);
    let icon = Rect::new(
        x,
        (rect.center_y() - icon_side * 0.5).round(),
        icon_side,
        icon_side,
    );
    icons::draw(
        p,
        Icon::for_kind(row.kind),
        icon,
        if selected { t.accent } else { t.text_muted },
        t.panel,
    );
    let name_x = icon.right() + p.m.px(size::GRID * 2.0);
    let name_rect = Rect::new(
        name_x,
        rect.y,
        (tl.names.right() - pad - name_x).max(0.0),
        rect.h,
    );
    let style = p.style(
        type_scale::BODY,
        if selected { t.text } else { t.text_body },
    );
    p.text_in(&row.name, name_rect, Align::Left, &style);

    // Lifetime bar: every layer lives for the whole composition today.
    let bar = Rect::new(
        x_of(0).round(),
        (rect.center_y() - bar_h * 0.5).round(),
        (x_of(duration - 1) - x_of(0)).round(),
        bar_h,
    );
    p.fill_rounded(
        bar,
        if selected {
            t.control_hover
        } else {
            t.track_bar
        },
        p.m.px(3.0),
    );
    if let (Some(&first), Some(&last)) = (row.keyframes.first(), row.keyframes.last()) {
        if last > first {
            let span = Rect::new(
                x_of(first).round(),
                bar.y,
                (x_of(last) - x_of(first)).round(),
                bar.h,
            );
            p.fill_rounded(span, t.track_bar_animated, p.m.px(3.0));
        }
        for &key in &row.keyframes {
            let marker = Rect::new(
                x_of(key) - diamond * 0.5,
                rect.center_y() - diamond * 0.5,
                diamond,
                diamond,
            );
            icons::draw(p, Icon::Diamond, marker, t.keyframe, t.app);
        }
    }
}

fn playhead(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let tl = v.layout.timeline;
    let duration = v.duration.max(1);
    let scrub = tl.scrub;
    let x = frame_to_x(v.frame, duration, f64::from(scrub.x), f64::from(scrub.w)).round() as f32;
    let area = Rect::new(tl.ruler.x, tl.ruler.y, tl.ruler.w, tl.ruler.h + tl.tracks.h);
    p.push_clip(area);
    let w = p.m.hairline().max(p.m.px(1.5).floor()).max(1.0);
    p.fill(
        Rect::new(x - (w * 0.5).floor(), tl.ruler.y, w, area.h),
        t.accent,
    );

    let (head, label) = head_rect(p, v);
    let style = p.style(type_scale::CAPTION, t.text).medium();
    p.fill_rounded(head, t.accent, p.m.px(size::RADIUS_SM));
    p.text_in(&label, head, Align::Center, &style);
    p.pop_clip();
}

/// The playhead tab on the ruler and its frame label.
fn head_rect(p: &Painter<'_>, v: &View<'_>) -> (Rect, String) {
    let tl = v.layout.timeline;
    let duration = v.duration.max(1);
    let x = frame_to_x(
        v.frame,
        duration,
        f64::from(tl.scrub.x),
        f64::from(tl.scrub.w),
    )
    .round() as f32;
    let style = p.style(type_scale::CAPTION, p.theme.text).medium();
    let label = v.frame.to_string();
    let w = (p.measure(&label, &style) + p.m.px(size::GRID * 3.0)).round();
    let h = p.m.px(18.0);
    let head_x = (x - w * 0.5)
        .clamp(tl.ruler.x, tl.ruler.right() - w)
        .round();
    (
        Rect::new(head_x, (tl.ruler.center_y() - h * 0.5).round(), w, h),
        label,
    )
}

#[cfg(test)]
mod tests {
    use super::minor_step;

    #[test]
    fn minor_steps_divide_the_major_step() {
        assert_eq!(minor_step(60), 10);
        assert_eq!(minor_step(30), 5);
        assert_eq!(minor_step(10), 2);
        assert_eq!(minor_step(5), 1);
        assert_eq!(minor_step(1), 1);
        assert_eq!(minor_step(7), 7);
    }
}

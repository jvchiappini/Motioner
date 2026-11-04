//! Layers panel: the scene outline with kind icons, indentation guides,
//! selection, visibility/lock toggles, expansion chevrons and the collapsible
//! orphans section at the bottom.

use crate::icons::{self, Icon};
use crate::layout::{Hit, Rect};
use crate::outline::OutlineRow;
use crate::paint::Painter;
use crate::panels::View;
use crate::text::Align;
use crate::theme::{over, size, type_scale};

/// Draws the layers panel.
pub fn draw(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let panel = v.layout.layers;
    p.fill(panel.rect, t.panel);
    let count = (!v.outline.is_empty()).then_some(v.layer_count);
    crate::panels::assets::tabs(p, v, count);
    filter_field(p, v);
    p.vline(
        panel.rect.right() - p.m.hairline(),
        panel.rect.y,
        panel.rect.bottom(),
        t.hairline,
    );

    let body = panel.body;
    if v.outline.is_empty() {
        empty_state(p, body, v);
        return;
    }

    p.push_clip(body);
    let first = visible_start(v.layout, v.layer_scroll, body);
    for (position, &index) in v.layer_rows.iter().enumerate().skip(first) {
        let rect = v.layout.layer_row(position, v.layer_scroll);
        if rect.y > body.bottom() {
            break;
        }
        if let Some(row) = v.outline.get(index) {
            draw_row(p, v, index, row, rect);
        }
    }
    orphans(p, v, body);
    p.pop_clip();
}

/// The first visible layer row that can intersect the body.
fn visible_start(layout: &crate::layout::Layout, scroll: f32, body: Rect) -> usize {
    if layout.row_h <= 0.0 {
        return 0;
    }
    let offset = body.y - layout.layers.body.y - layout.metrics.px(size::GRID) + scroll;
    usize::try_from((offset / layout.row_h).floor().max(0.0) as i64).unwrap_or(0)
}

/// The filter box in the layers header; typing goes to `FieldTarget::Filter`.
fn filter_field(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let shy = v.layout.shy_toggle();
    p.fill_rounded(
        shy,
        if v.show_shy { t.control } else { t.raised },
        p.m.px(size::RADIUS_SM),
    );
    p.text_in(
        "Y",
        shy,
        Align::Center,
        &p.style(
            type_scale::CAPTION,
            if v.show_shy {
                t.warning
            } else {
                t.text_disabled
            },
        ),
    );
    let rect = v.layout.layer_filter();
    let editing = v
        .editing
        .is_some_and(|editing| matches!(editing.target, crate::inspect::FieldTarget::Filter));
    p.fill_rounded(
        rect,
        if editing { t.control } else { t.raised },
        p.m.px(size::RADIUS_SM),
    );
    let style = p.style(
        type_scale::CAPTION,
        if editing {
            t.text
        } else if v.filter.is_empty() {
            t.text_disabled
        } else {
            t.text_body
        },
    );
    let text: &str = if editing {
        v.editing.map_or("", |editing| editing.editor.text())
    } else if v.filter.is_empty() {
        "Filter layers"
    } else {
        v.filter
    };
    p.text_in(
        text,
        rect.inset_xy(p.m.px(size::GRID * 1.5), 0.0),
        Align::Left,
        &style,
    );
}

/// One outline row: highlight, guides, chevron, kind icon, name, detail and
/// the eye/lock actions.
fn draw_row(p: &mut Painter<'_>, v: &View<'_>, index: usize, row: &OutlineRow, rect: Rect) {
    let t = *p.theme;
    let pad = p.m.px(size::PAD);
    let indent = p.m.px(size::INDENT);
    let chevron_side = p.m.px(12.0);
    let selected = v.selected == Some(index);
    let hovered = matches!(
        v.hover,
        Some(
            Hit::LayerRow(i)
                | Hit::LayerEye(i)
                | Hit::LayerLock(i)
                | Hit::LayerSolo(i)
                | Hit::LayerShy(i)
                | Hit::LayerChevron(i)
        ) if i == index
    );
    let (visible, locked) = v.layer_states.get(index).copied().unwrap_or((true, true));
    let (solo, shy) = v.layer_flags.get(index).copied().unwrap_or((false, false));
    let highlight = rect.inset_xy(p.m.px(size::GRID), 0.0);
    if selected {
        p.fill_rounded(highlight, t.selection, p.m.px(size::RADIUS_SM));
    } else if hovered {
        p.fill_rounded(highlight, t.row_hover, p.m.px(size::RADIUS_SM));
    }
    draw_guides(p, row, rect, pad, indent, chevron_side);
    let x = rect.x + pad + row.depth as f32 * indent;
    let cy = rect.center_y();
    draw_chevron(
        p,
        v,
        index,
        row,
        Rect::new(
            x,
            (cy - chevron_side * 0.5).round(),
            chevron_side,
            chevron_side,
        ),
    );
    let icon_side = p.m.px(size::ICON);
    let icon = Rect::new(
        x + chevron_side + p.m.px(2.0),
        (cy - icon_side * 0.5).round(),
        icon_side,
        icon_side,
    );
    let ink = if !visible {
        t.text_disabled
    } else if selected {
        t.accent
    } else {
        t.text_muted
    };
    icons::draw(p, Icon::for_kind(row.kind), icon, ink, t.panel);
    let mut icon_right = icon.right();
    // A masked node and a mask source both carry the mask badge; a source that
    // is a child is already indented under the node that uses it.
    if row.mask_role.shows_badge() {
        let badge = Rect::new(icon_right + p.m.px(2.0), icon.y, icon_side, icon_side);
        icons::draw(p, Icon::Mask, badge, ink, t.panel);
        icon_right = badge.right();
    }
    draw_labels(p, v, row, rect, icon_right, selected, visible || !locked);
    if hovered || selected || !visible || locked || solo || shy {
        draw_actions(p, v, index, rect, selected, visible, locked, solo, shy);
    }
}

/// One hairline per ancestor level.
fn draw_guides(
    p: &mut Painter<'_>,
    row: &OutlineRow,
    rect: Rect,
    pad: f32,
    indent: f32,
    chevron_side: f32,
) {
    for level in 0..row.depth {
        let gx = (rect.x + pad + level as f32 * indent + chevron_side * 0.5).round();
        p.vline(gx, rect.y, rect.bottom(), p.theme.hairline);
    }
}

fn draw_chevron(p: &mut Painter<'_>, v: &View<'_>, index: usize, row: &OutlineRow, rect: Rect) {
    if !row.has_children {
        return;
    }
    let t = *p.theme;
    let open = v.expanded.contains(&index);
    icons::draw(
        p,
        if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        rect,
        if v.hover == Some(Hit::LayerChevron(index)) {
            t.text
        } else {
            t.text_muted
        },
        t.panel,
    );
}

fn draw_labels(
    p: &mut Painter<'_>,
    v: &View<'_>,
    row: &OutlineRow,
    rect: Rect,
    icon_right: f32,
    selected: bool,
    readable: bool,
) {
    let t = *p.theme;
    let text_x = icon_right + p.m.px(size::GRID * 2.0);
    let detail_ink = if selected {
        t.text_muted
    } else {
        t.text_disabled
    };
    let detail_style = p.style(type_scale::CAPTION, detail_ink);
    let gap = p.m.px(size::GRID * 2.0);
    let detail_measure = if row.detail.is_empty() {
        0.0
    } else {
        p.measure(&row.detail, &detail_style)
    };
    let actions_x = v.layout.layer_lock(rect).x;
    let available = (actions_x - p.m.px(size::GRID) - text_x).max(0.0);
    let (name_room, detail_room) = label_rooms(available, detail_measure, gap);
    let name_style = p.style(
        type_scale::BODY,
        if !readable {
            t.text_disabled
        } else if selected {
            t.text
        } else {
            t.text_body
        },
    );
    p.text_in(
        &row.name,
        Rect::new(text_x, rect.y, name_room, rect.h),
        Align::Left,
        &name_style,
    );
    if detail_room > 0.0 {
        // The detail starts right after the (possibly ellipsized) name.  It
        // gets its own clamped width so a long path can never run under the
        // eye/lock icons.
        let name_w = p.measure(&row.name, &name_style).min(name_room);
        let detail = Rect::new(text_x + name_w + gap, rect.y, detail_room, rect.h);
        p.text_in(&row.detail, detail, Align::Left, &detail_style);
    }
}

/// Splits the room between a layer name and its detail text.
///
/// The detail keeps at most half of the available width and the name always
/// gets the rest, so both are ellipsized before the trailing actions.  Returns
/// `(name_room, detail_room)`; the two never exceed `available` once the gap
/// between them is added.
fn label_rooms(available: f32, detail_measure: f32, gap: f32) -> (f32, f32) {
    if detail_measure <= 0.0 {
        return (available.max(0.0), 0.0);
    }
    let detail_room = (detail_measure + gap).min(available * 0.5);
    let name_room = (available - detail_room - gap).max(0.0);
    (name_room, detail_room)
}

/// Eye, lock, solo and shy stay visible (dimmed) on hidden or marked rows.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)] // one row's four actions
fn draw_actions(
    p: &mut Painter<'_>,
    v: &View<'_>,
    index: usize,
    rect: Rect,
    selected: bool,
    visible: bool,
    locked: bool,
    solo: bool,
    shy: bool,
) {
    let t = *p.theme;
    let surface = over(if selected { t.selection } else { t.panel }, t.panel);
    let eye = v.layout.layer_eye(rect);
    let eye_ink = if visible {
        if v.hover == Some(Hit::LayerEye(index)) {
            t.text
        } else {
            t.text_muted
        }
    } else {
        t.warning
    };
    icons::draw(p, Icon::Eye, eye.inset(p.m.px(2.0)), eye_ink, surface);
    let lock = v.layout.layer_lock(rect);
    let lock_ink = if locked {
        t.warning
    } else if v.hover == Some(Hit::LayerLock(index)) {
        t.text
    } else {
        t.text_muted
    };
    let glyph = if locked { Icon::Lock } else { Icon::Unlock };
    icons::draw(p, glyph, lock.inset(p.m.px(3.0)), lock_ink, surface);
    let solo_rect = v.layout.layer_solo(rect);
    let shy_rect = v.layout.layer_shy(rect);
    p.text_in(
        "S",
        solo_rect,
        Align::Center,
        &p.style(
            type_scale::CAPTION,
            if solo { t.accent } else { t.text_disabled },
        ),
    );
    p.text_in(
        "Y",
        shy_rect,
        Align::Center,
        &p.style(
            type_scale::CAPTION,
            if shy { t.warning } else { t.text_disabled },
        ),
    );
}
/// The collapsible "Orphans" section: overrides whose node is gone.
fn orphans(p: &mut Painter<'_>, v: &View<'_>, body: Rect) {
    if v.orphans.is_empty() {
        return;
    }
    let t = *p.theme;
    let header = v.layout.orphans_header(v.layer_rows.len(), v.layer_scroll);
    if header.bottom() < body.y || header.y > body.bottom() {
        return;
    }
    p.hline(header.x, header.right(), header.y, t.hairline);
    let caption = crate::widgets::caption(p, t.warning);
    p.text_in(
        &format!("ORPHANS ({})", v.orphans.len()),
        header.inset_xy(p.m.px(size::PAD), 0.0),
        Align::Left,
        &caption,
    );
    let title = p.style(type_scale::CAPTION, t.text_disabled);
    p.text_in(
        "kept, never rendered",
        header.inset_xy(p.m.px(size::PAD), 0.0),
        Align::Right,
        &title,
    );
    for (index, orphan) in v.orphans.iter().enumerate() {
        let rect = v
            .layout
            .orphan_row(index, v.layer_rows.len(), v.layer_scroll);
        if rect.bottom() < body.y || rect.y > body.bottom() {
            continue;
        }
        let hovered = v.hover == Some(Hit::OrphanRow(index));
        if hovered {
            p.fill_rounded(
                rect.inset_xy(p.m.px(size::GRID), 0.0),
                t.row_hover,
                p.m.px(size::RADIUS_SM),
            );
        }
        let id_style = p.style(type_scale::CAPTION, t.text_muted);
        p.text_in(
            &orphan.id,
            Rect::new(rect.x + p.m.px(size::PAD), rect.y, rect.w * 0.5, rect.h),
            Align::Left,
            &id_style,
        );
        let prop_style = p.style(type_scale::CAPTION, t.text_disabled);
        p.text_in(
            &orphan.property,
            Rect::new(rect.x + rect.w * 0.5, rect.y, rect.w * 0.4, rect.h),
            Align::Left,
            &prop_style,
        );
        let delete = v.layout.orphan_delete(rect);
        if orphan.suggestion.is_some() {
            let rebind = v.layout.orphan_rebind(rect);
            let state = v.interaction(Hit::OrphanRebind(index));
            let ink = if state.hovered {
                t.accent
            } else {
                t.text_muted
            };
            icons::draw(p, Icon::Check, rebind.inset(p.m.px(4.0)), ink, t.panel);
        }
        let state = v.interaction(Hit::OrphanDelete(index));
        let ink = if state.hovered {
            t.danger
        } else {
            t.text_disabled
        };
        icons::draw(p, Icon::Error, delete.inset(p.m.px(4.0)), ink, t.panel);
    }
}

fn empty_state(p: &mut Painter<'_>, body: Rect, v: &View<'_>) {
    let t = *p.theme;
    let (title, hint) = if v.document.is_some() {
        (
            "No layers yet",
            format!("Add shapes or text in {}", v.file_name),
        )
    } else if v.has_error() {
        ("Nothing to show", "The file has an error".to_owned())
    } else {
        ("Loading", String::new())
    };
    let line = p.m.px(18.0);
    let top = body.y + p.m.px(size::PAD * 2.0);
    let inner = body.inset_xy(p.m.px(size::PAD), 0.0);
    let title_style = p.style(type_scale::BODY, t.text_muted).medium();
    p.text_in(
        title,
        Rect::new(inner.x, top, inner.w, line),
        Align::Left,
        &title_style,
    );
    if !hint.is_empty() {
        let hint_style = p.style(type_scale::CAPTION, t.text_disabled);
        p.text_in(
            &hint,
            Rect::new(inner.x, top + line, inner.w, line),
            Align::Left,
            &hint_style,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::label_rooms;

    #[test]
    fn a_long_name_never_reaches_the_row_actions() {
        // 300 px available, a 500 px detail and an 8 px gap: the detail is
        // clamped to half the row and the name keeps the rest, so the two
        // rectangles plus the gap stay inside the available width.
        let (name_room, detail_room) = label_rooms(300.0, 500.0, 8.0);
        assert!(name_room > 0.0, "the name still gets room");
        assert!(detail_room > 0.0, "the detail is shown, clamped");
        assert!(
            name_room + detail_room + 8.0 <= 300.0 + f32::EPSILON,
            "labels must end before the eye/lock icons: {name_room} + {detail_room} + 8 > 300"
        );
    }

    #[test]
    fn a_short_detail_leaves_the_rest_of_the_name() {
        let (name_room, detail_room) = label_rooms(300.0, 40.0, 8.0);
        assert!((detail_room - 48.0).abs() < 1e-6);
        assert!((name_room - 244.0).abs() < 1e-6);
    }

    #[test]
    fn no_detail_uses_the_whole_width() {
        let (name_room, detail_room) = label_rooms(200.0, 0.0, 8.0);
        assert!((name_room - 200.0).abs() < 1e-6);
        assert!(detail_room.abs() < 1e-6);
    }

    #[test]
    fn a_zero_width_row_does_not_panic() {
        let (name_room, detail_room) = label_rooms(0.0, 100.0, 8.0);
        assert!(name_room.abs() < 1e-6);
        assert!(detail_room.abs() < 1e-6);
    }
}

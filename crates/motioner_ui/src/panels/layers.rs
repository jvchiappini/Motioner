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
    let count = (!v.outline.is_empty()).then(|| v.outline.len().to_string());
    crate::widgets::panel_header(p, panel.header, "LAYERS", count.as_deref());
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
    for (index, row) in v.outline.iter().enumerate() {
        let rect = v.layout.layer_row(index, v.layer_scroll);
        if rect.bottom() >= body.y && rect.y <= body.bottom() {
            draw_row(p, v, index, row, rect);
        }
    }
    orphans(p, v, body);
    p.pop_clip();
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
                | Hit::LayerChevron(i)
        ) if i == index
    );
    let (visible, locked) = v.layer_states.get(index).copied().unwrap_or((true, true));
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
    if hovered || selected || !visible || locked {
        draw_actions(p, v, index, rect, selected, visible, locked);
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
    let detail_w = if row.detail.is_empty() {
        0.0
    } else {
        p.measure(&row.detail, &detail_style) + p.m.px(size::GRID * 2.0)
    };
    let actions_x = v.layout.layer_lock(rect).x;
    let name_room = (actions_x - p.m.px(size::GRID) - detail_w - text_x).max(0.0);
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
    if detail_w > 0.0 {
        let name_w = p.measure(&row.name, &name_style).min(name_room);
        let detail = Rect::new(
            text_x + name_w + p.m.px(size::GRID * 2.0),
            rect.y,
            detail_w,
            rect.h,
        );
        p.text_in(&row.detail, detail, Align::Left, &detail_style);
    }
}

/// Eye and lock stay visible (dimmed) on hidden or locked rows.
fn draw_actions(
    p: &mut Painter<'_>,
    v: &View<'_>,
    index: usize,
    rect: Rect,
    selected: bool,
    visible: bool,
    locked: bool,
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
}
/// The collapsible "Orphans" section: overrides whose node is gone.
fn orphans(p: &mut Painter<'_>, v: &View<'_>, body: Rect) {
    if v.orphans.is_empty() {
        return;
    }
    let t = *p.theme;
    let header = v.layout.orphans_header(v.outline.len(), v.layer_scroll);
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
        let rect = v.layout.orphan_row(index, v.outline.len(), v.layer_scroll);
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

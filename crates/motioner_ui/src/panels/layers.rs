//! Layers panel: the scene outline with kind icons, indentation guides,
//! hover and selection, and a visibility eye (display only for now).

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
    p.pop_clip();
}

/// One outline row: highlight, guides, chevron, kind icon, name, detail and
/// the hover actions.
fn draw_row(p: &mut Painter<'_>, v: &View<'_>, index: usize, row: &OutlineRow, rect: Rect) {
    let t = *p.theme;
    let pad = p.m.px(size::PAD);
    let indent = p.m.px(size::INDENT);
    let icon_side = p.m.px(size::ICON);
    let chevron_side = p.m.px(12.0);
    let selected = v.selected == Some(index);
    let hovered = v.hovered_row() == Some(index);
    let highlight = rect.inset_xy(p.m.px(size::GRID), 0.0);
    if selected {
        p.fill_rounded(highlight, t.selection, p.m.px(size::RADIUS_SM));
    } else if hovered {
        p.fill_rounded(highlight, t.row_hover, p.m.px(size::RADIUS_SM));
    }

    // Indentation guides: one hairline per ancestor level.
    let base = rect.x + pad;
    for level in 0..row.depth {
        let gx = (base + level as f32 * indent + chevron_side * 0.5).round();
        p.vline(gx, rect.y, rect.bottom(), t.hairline);
    }

    let x = base + row.depth as f32 * indent;
    let cy = rect.center_y();
    if row.has_children {
        let chevron = Rect::new(
            x,
            (cy - chevron_side * 0.5).round(),
            chevron_side,
            chevron_side,
        );
        icons::draw(p, Icon::ChevronDown, chevron, t.text_muted, t.panel);
    }
    let icon = Rect::new(
        x + chevron_side + p.m.px(2.0),
        (cy - icon_side * 0.5).round(),
        icon_side,
        icon_side,
    );
    let ink = if selected { t.accent } else { t.text_muted };
    icons::draw(p, Icon::for_kind(row.kind), icon, ink, t.panel);

    let text_x = icon.right() + p.m.px(size::GRID * 2.0);
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
        if selected { t.text } else { t.text_body },
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

    // The actions only appear on the row being pointed at or selected, so
    // the list stays quiet (they are display-only until editing lands).
    if hovered || selected {
        let surface = over(if selected { t.selection } else { t.row_hover }, t.panel);
        let ink = |hit| {
            if v.interaction(hit).hovered {
                t.text
            } else {
                t.text_muted
            }
        };
        let eye = v.layout.layer_eye(rect);
        icons::draw(
            p,
            Icon::Eye,
            eye.inset(p.m.px(2.0)),
            ink(Hit::LayerEye(index)),
            surface,
        );
        let lock = v.layout.layer_lock(rect);
        icons::draw(
            p,
            Icon::Lock,
            lock.inset(p.m.px(3.0)),
            ink(Hit::LayerLock(index)),
            surface,
        );
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

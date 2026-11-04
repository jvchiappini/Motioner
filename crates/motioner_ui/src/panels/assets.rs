//! Assets panel: the Layers/Assets tab strip, toolbar and the virtualised
//! list or grid of project assets.

use crate::assets::{AssetKindUi, AssetView};
use crate::layout::{Hit, LeftTab, Rect};
use crate::paint::Painter;
use crate::panels::View;
use crate::text::Align;
use crate::theme::{size, type_scale};

/// Draws the two tabs and the trailing count in the left panel header.
pub fn tabs(p: &mut Painter<'_>, v: &View<'_>, count: Option<&str>) {
    let t = *p.theme;
    let header = v.layout.layers.header;
    p.fill(header, t.panel);
    let style = p.style(type_scale::CAPTION, t.text_body).medium();
    for tab in LeftTab::ALL {
        let rect = v.layout.left_tab(tab);
        let state = v.interaction(Hit::LeftTab(tab));
        let active = v.left_tab == tab;
        let fill = if active {
            t.raised
        } else if state.hovered {
            t.row_hover
        } else {
            t.panel
        };
        p.fill_rounded(rect, fill, p.m.px(size::RADIUS_SM));
        if active {
            p.stroke_rounded(rect, t.accent, p.m.px(size::RADIUS_SM), p.m.px(1.0));
        }
        let ink = if active { t.text } else { t.text_muted };
        let style = style.color(ink);
        p.text_in(tab.label(), rect, Align::Center, &style);
    }
    if let Some(count) = count {
        let muted = p.style(type_scale::CAPTION, t.text_disabled);
        p.text_in(
            count,
            Rect::new(header.x, header.y, header.w - p.m.px(size::PAD), header.h),
            Align::Right,
            &muted,
        );
    }
    p.hline(header.x, header.right(), header.bottom(), t.hairline);
}

/// Draws the assets panel.
pub fn draw(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let panel = v.layout.layers;
    p.fill(panel.rect, t.panel);
    tabs(p, v, None);
    toolbar(p, v);
    p.vline(
        panel.rect.right() - p.m.hairline(),
        panel.rect.y,
        panel.rect.bottom(),
        t.hairline,
    );

    let body = panel.body;
    if v.assets.entries.is_empty() {
        let style = p.style(type_scale::BODY, t.text_muted);
        p.text_in(
            "Drop files here or press +",
            Rect::new(
                body.x + p.m.px(12.0),
                body.y + p.m.px(16.0),
                body.w,
                p.m.px(20.0),
            ),
            Align::Left,
            &style,
        );
        return;
    }
    p.push_clip(body);
    match v.assets.view() {
        AssetView::List => draw_list(p, v, body),
        AssetView::Grid => draw_grid(p, v, body),
    }
    p.pop_clip();
}

fn toolbar(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let (add, view, unused) = v.layout.asset_toolbar();
    let add_state = v.interaction(Hit::AssetAdd);
    p.fill_rounded(
        add,
        if add_state.hovered {
            t.row_hover
        } else {
            t.raised
        },
        p.m.px(size::RADIUS_SM),
    );
    let plus = p.style(type_scale::BODY, t.text_body).medium();
    p.text_in("+", add, Align::Center, &plus);

    let view_state = v.interaction(Hit::AssetView);
    p.fill_rounded(
        view,
        if view_state.hovered {
            t.row_hover
        } else {
            t.raised
        },
        p.m.px(size::RADIUS_SM),
    );
    let label = match v.assets.view() {
        AssetView::List => "GRID",
        AssetView::Grid => "LIST",
    };
    let style = p.style(type_scale::CAPTION, t.text_muted);
    p.text_in(label, view, Align::Center, &style);

    let unused_state = v.interaction(Hit::AssetRemoveUnused);
    p.fill_rounded(
        unused,
        if unused_state.hovered {
            t.row_hover
        } else {
            t.raised
        },
        p.m.px(size::RADIUS_SM),
    );
    let style = p.style(type_scale::CAPTION, t.text_muted);
    p.text_in("CLEAN", unused, Align::Center, &style);
}

fn draw_list(p: &mut Painter<'_>, v: &View<'_>, body: Rect) {
    let t = *p.theme;
    let row_h = v.layout.row_h;
    if row_h <= 0.0 {
        return;
    }
    let offset = (body.y - v.layout.layers.body.y - p.m.px(size::GRID) + v.layer_scroll).max(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let first = (offset / row_h).floor() as usize;
    let count = v.assets.visible_count(v.filter);
    for position in first..count {
        let rect = v.layout.asset_row(position, v.layer_scroll);
        if rect.y > body.bottom() {
            break;
        }
        if rect.bottom() < body.y {
            continue;
        }
        let Some(index) = v.assets.list_index(position, v.filter) else {
            continue;
        };
        let Some(entry) = v.assets.entries.get(index) else {
            continue;
        };
        let selected = v.assets.selected == Some(index);
        let state = v.interaction(Hit::AssetRow(index));
        if selected {
            p.fill_rounded(rect, t.selection, 0.0);
        } else if state.hovered {
            p.fill_rounded(rect, t.row_hover, 0.0);
        }
        let badge = Rect::new(
            rect.x + p.m.px(size::PAD),
            rect.center_y() - p.m.px(7.0),
            p.m.px(24.0),
            p.m.px(14.0),
        );
        kind_badge(p, badge, entry.kind, &t);
        let name = p.style(type_scale::BODY, t.text_body).medium();
        p.text_in(
            &entry.key,
            Rect::new(
                badge.right() + p.m.px(size::PAD),
                rect.y,
                rect.w - p.m.px(140.0),
                rect.h,
            ),
            Align::Left,
            &name,
        );
        let detail = p.style(type_scale::CAPTION, t.text_muted);
        p.text_in(
            v.assets.detail(index),
            Rect::new(rect.right() - p.m.px(120.0), rect.y, p.m.px(112.0), rect.h),
            Align::Right,
            &detail,
        );
    }
}

fn draw_grid(p: &mut Painter<'_>, v: &View<'_>, body: Rect) {
    let t = *p.theme;
    let step = v.layout.asset_grid_step();
    if step <= 0.0 {
        return;
    }
    let offset = (body.y - v.layout.layers.body.y - p.m.px(size::GRID) + v.layer_scroll).max(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let first_row = (offset / step).floor() as usize;
    let count = v.assets.visible_count(v.filter);
    for position in first_row * 2..count {
        let rect = v.layout.asset_cell(position, v.layer_scroll);
        if rect.y > body.bottom() {
            break;
        }
        if rect.bottom() < body.y {
            continue;
        }
        let Some(index) = v.assets.list_index(position, v.filter) else {
            continue;
        };
        let Some(entry) = v.assets.entries.get(index) else {
            continue;
        };
        let state = v.interaction(Hit::AssetCell(index));
        let fill = if v.assets.selected == Some(index) {
            t.selection
        } else if state.hovered {
            t.row_hover
        } else {
            t.raised
        };
        p.fill_rounded(rect, fill, p.m.px(size::RADIUS_MD));
        let strip = Rect::new(
            rect.x + p.m.px(8.0),
            rect.y + p.m.px(8.0),
            rect.w - p.m.px(16.0),
            p.m.px(28.0),
        );
        match entry.kind {
            AssetKindUi::Audio if !entry.peaks.is_empty() => {
                waveform(p, strip, &entry.peaks, t.accent);
            }
            kind => {
                let color = kind_color(kind, &t);
                p.fill_rounded(strip, color, p.m.px(size::RADIUS_SM));
                let label = p.style(type_scale::CAPTION, t.pasteboard);
                p.text_in(kind.label(), strip, Align::Center, &label);
            }
        }
        let name = p.style(type_scale::BODY, t.text_body).medium();
        p.text_in(
            &entry.key,
            Rect::new(
                rect.x + p.m.px(8.0),
                strip.bottom() + p.m.px(4.0),
                rect.w - p.m.px(16.0),
                p.m.px(18.0),
            ),
            Align::Left,
            &name,
        );
        let detail = p.style(type_scale::CAPTION, t.text_muted);
        p.text_in(
            v.assets.detail(index),
            Rect::new(
                rect.x + p.m.px(8.0),
                strip.bottom() + p.m.px(20.0),
                rect.w - p.m.px(16.0),
                p.m.px(14.0),
            ),
            Align::Left,
            &detail,
        );
    }
}

fn kind_badge(p: &mut Painter<'_>, rect: Rect, kind: AssetKindUi, t: &crate::theme::Theme) {
    p.fill_rounded(rect, kind_color(kind, t), p.m.px(size::RADIUS_SM));
    let style = p.style(type_scale::CAPTION, t.pasteboard);
    let label = match kind {
        AssetKindUi::Image => "IMG",
        AssetKindUi::Font => "TTF",
        AssetKindUi::Audio => "AUD",
        AssetKindUi::Video => "VID",
    };
    p.text_in(label, rect, Align::Center, &style);
}

fn kind_color(kind: AssetKindUi, t: &crate::theme::Theme) -> [f32; 4] {
    match kind {
        AssetKindUi::Image => t.accent,
        AssetKindUi::Font => t.keyframe,
        AssetKindUi::Audio => t.success,
        AssetKindUi::Video => t.warning,
    }
}

fn waveform(p: &mut Painter<'_>, rect: Rect, peaks: &[f32], color: [f32; 4]) {
    let count = peaks.len();
    if count == 0 || rect.w <= 0.0 {
        return;
    }
    let step = (rect.w / count as f32).max(1.0);
    let mut color = color;
    color[3] = 0.75;
    for (index, peak) in peaks.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = rect.x + index as f32 * step;
        let height = (rect.h * peak.clamp(0.0, 1.0)).max(1.0);
        p.fill(
            Rect::new(
                x,
                rect.center_y() - height * 0.5,
                (step - 1.0).max(1.0),
                height,
            ),
            color,
        );
    }
}

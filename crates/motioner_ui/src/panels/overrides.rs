//! The pending-overrides chip, list and the one-time convert-on-open banner.
//!
//! The model (rows and command text) lives in
//! [`crate::overrides_panel`](crate::OverrideRow); this module paints it.

use crate::layout::Rect;
use crate::paint::{Painter, PanelKind, PanelRevision};
use crate::panels::View;
use crate::text::Align;
use crate::theme::{size, type_scale};

/// Draws the "N edits as overrides" chip in the top bar, when there are any.
pub fn draw_chip(p: &mut Painter<'_>, v: &View<'_>) {
    if v.override_rows.is_empty() {
        return;
    }
    let t = *p.theme;
    let chip = v.layout.overrides_chip;
    let fill = if v.overrides_open {
        t.control
    } else {
        t.warning
    };
    p.fill_rounded(chip, fill, chip.h * 0.5);
    let style = p.style(type_scale::CAPTION, t.text).medium();
    p.text_in(
        &format!("{} edits as overrides", v.override_rows.len()),
        chip,
        Align::Center,
        &style,
    );
}

/// Hash of the pending-overrides rows.
fn rows_revision(mut rev: PanelRevision, v: &View<'_>) -> PanelRevision {
    rev = rev.usize(v.override_rows.len());
    for row in v.override_rows {
        rev = rev.str(&row.command);
        match &row.reason {
            Some(reason) => rev = rev.bool(true).str(reason),
            None => rev = rev.bool(false),
        }
    }
    rev
}

/// Draws the pending-overrides list.
pub fn draw(p: &mut Painter<'_>, v: &View<'_>) {
    let list = v.layout.overrides_list();
    let key = p.panel_key(
        PanelKind::OverridesList,
        rows_revision(PanelRevision::new(), v).finish(),
        list,
    );
    p.panel(key, |p| draw_body(p, v));
}

fn draw_body(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let list = v.layout.overrides_list();
    if list.w <= 0.0 || list.h <= 0.0 {
        return;
    }
    p.shadow(
        list,
        p.m.px(8.0),
        p.m.px(20.0),
        p.m.px(6.0),
        [0.0, 0.0, 0.0, 0.65],
    );
    p.fill_rounded(list, t.panel, p.m.px(6.0));
    p.stroke_rounded(list, t.hairline, p.m.px(6.0), p.m.px(1.0));
    let title = p.style(type_scale::EMPHASIS, t.text).medium();
    p.text_in(
        &format!("{} edits saved as overrides", v.override_rows.len()),
        Rect::new(
            list.x + p.m.px(size::PAD),
            list.y + p.m.px(8.0),
            list.w - p.m.px(size::PAD * 8.0),
            p.m.px(22.0),
        ),
        Align::Left,
        &title,
    );
    let button = v.layout.overrides_close();
    p.fill_rounded(button, t.raised, p.m.px(size::RADIUS_SM));
    let close = p.style(type_scale::CAPTION, t.text_body);
    p.text_in("×", button, Align::Center, &close);
    let all = v.layout.overrides_convert_all();
    p.fill_rounded(all, t.accent, p.m.px(size::RADIUS_SM));
    let all_style = p.style(type_scale::CAPTION, t.text);
    p.text_in("Convert all", all, Align::Center, &all_style);

    let hint = p.style(type_scale::CAPTION, t.text_muted);
    let id_style = p.style(type_scale::CAPTION, t.text_body);
    let reason_style = p.style(type_scale::CAPTION, t.warning);
    let convert_style = p.style(type_scale::CAPTION, t.text);
    for (index, row) in v.override_rows.iter().enumerate() {
        let rect = v.layout.overrides_row(index);
        if rect.bottom() > list.bottom() - p.m.px(size::PAD) {
            break;
        }
        let convert = v.layout.overrides_convert(rect);
        p.text_in(
            &row.command,
            Rect::new(
                rect.x,
                rect.y,
                (convert.x - rect.x - p.m.px(size::GRID)).max(0.0),
                p.m.px(16.0),
            ),
            Align::Left,
            &id_style,
        );
        if let Some(reason) = &row.reason {
            p.text_in(
                reason,
                Rect::new(
                    rect.x,
                    rect.y + p.m.px(14.0),
                    (convert.x - rect.x - p.m.px(size::GRID)).max(0.0),
                    p.m.px(15.0),
                ),
                Align::Left,
                &reason_style,
            );
        }
        p.fill_rounded(convert, t.raised, p.m.px(size::RADIUS_SM));
        p.text_in("Convert", convert, Align::Center, &convert_style);
    }
    if v.override_rows.is_empty() {
        p.text_in(
            "No edits are waiting as overrides.",
            Rect::new(
                list.x + p.m.px(size::PAD),
                list.y + p.m.px(44.0),
                list.w - p.m.px(size::PAD * 2.0),
                p.m.px(18.0),
            ),
            Align::Left,
            &hint,
        );
    }
}

/// Draws the one-time convert-on-open banner.
pub fn draw_banner(p: &mut Painter<'_>, v: &View<'_>) {
    let banner = v.layout.bake_banner();
    let key = p.panel_key(
        PanelKind::OverridesBanner,
        rows_revision(PanelRevision::new(), v).finish(),
        banner,
    );
    p.panel(key, |p| draw_banner_body(p, v));
}

fn draw_banner_body(p: &mut Painter<'_>, v: &View<'_>) {
    let t = *p.theme;
    let banner = v.layout.bake_banner();
    if banner.w <= 0.0 {
        return;
    }
    p.fill_rounded(banner, t.raised, p.m.px(size::RADIUS_SM));
    p.stroke_rounded(banner, t.hairline, p.m.px(size::RADIUS_SM), p.m.px(1.0));
    let style = p.style(type_scale::CAPTION, t.text_body);
    p.text_in(
        &format!(
            "This file has {} edits saved as overrides. Convert them to code?",
            v.override_rows.len()
        ),
        Rect::new(
            banner.x + p.m.px(size::PAD),
            banner.y,
            (v.layout.bake_convert().x - banner.x - p.m.px(size::PAD * 2.0)).max(0.0),
            banner.h,
        ),
        Align::Left,
        &style,
    );
    for (rect, label) in [
        (v.layout.bake_convert(), "Convert"),
        (v.layout.bake_dismiss(), "Dismiss"),
    ] {
        p.fill_rounded(rect, t.control, p.m.px(size::RADIUS_SM));
        let button = p.style(type_scale::CAPTION, t.text);
        p.text_in(label, rect, Align::Center, &button);
    }
}

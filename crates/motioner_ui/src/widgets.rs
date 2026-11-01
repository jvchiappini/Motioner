//! Reusable chrome widgets drawn through [`Painter`].
//!
//! Widgets are immediate-mode drawing functions: the caller owns the state
//! (hovered, pressed, toggled) and passes it in as an [`Interaction`]. Layout
//! decides where a widget goes; hit testing lives in [`crate::layout`].

use crate::icons::{self, Icon};
use crate::layout::Rect;
use crate::paint::Painter;
use crate::text::{Align, TextStyle};
use crate::theme::{over, size, type_scale};

/// Pointer and toggle state of one widget.
///
/// The flags are independent visual inputs (a toggle can be hovered and
/// pressed while on), not states of one machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Interaction {
    /// The pointer is over the widget.
    pub hovered: bool,
    /// The primary button went down on the widget and is still held.
    pub pressed: bool,
    /// The widget cannot be used.
    pub disabled: bool,
    /// A toggle is on.
    pub active: bool,
}

/// Visual weight of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// Transparent until hovered.
    Ghost,
    /// The single primary action: filled with the accent.
    Primary,
}

/// An icon-only button with hover, pressed, toggled and disabled states.
pub fn icon_button(
    p: &mut Painter<'_>,
    rect: Rect,
    icon: Icon,
    kind: ButtonKind,
    state: Interaction,
) {
    let t = *p.theme;
    let radius = p.m.px(size::RADIUS_MD);
    let (surface, ink) = match kind {
        ButtonKind::Primary => {
            let fill = if state.disabled {
                t.control
            } else if state.pressed {
                t.accent
            } else if state.hovered {
                t.accent_hover
            } else {
                t.accent
            };
            p.fill_rounded(rect, fill, rect.w * 0.5);
            (
                fill,
                if state.disabled {
                    t.text_disabled
                } else {
                    t.text
                },
            )
        }
        ButtonKind::Ghost => {
            let fill = if state.disabled {
                [0.0; 4]
            } else if state.pressed {
                t.control_pressed
            } else if state.active {
                t.selection
            } else if state.hovered {
                t.control
            } else {
                [0.0; 4]
            };
            p.fill_rounded(rect, fill, radius);
            let ink = if state.disabled {
                t.text_disabled
            } else if state.active {
                t.accent
            } else if state.hovered {
                t.text
            } else {
                t.text_muted
            };
            (over(fill, t.panel), ink)
        }
    };
    let icon_side = p.m.px(size::ICON);
    let icon_rect = Rect::new(
        (rect.center_x() - icon_side * 0.5).round(),
        (rect.center_y() - icon_side * 0.5).round(),
        icon_side,
        icon_side,
    );
    icons::draw(p, icon, icon_rect, ink, surface);
}

/// A panel header: title in small caps on the panel surface, an optional
/// trailing count, and a hairline underneath.
pub fn panel_header(p: &mut Painter<'_>, rect: Rect, title: &str, trailing: Option<&str>) {
    let t = *p.theme;
    p.fill(rect, t.panel);
    let pad = p.m.px(size::PAD);
    let style = caption(p, t.text_muted);
    let inner = rect.inset_xy(pad, 0.0);
    p.text_in(title, inner, Align::Left, &style);
    if let Some(trailing) = trailing {
        let style = p.style(type_scale::CAPTION, t.text_disabled);
        p.text_in(trailing, inner, Align::Right, &style);
    }
    p.hline(
        rect.x,
        rect.right(),
        rect.bottom() - p.m.hairline(),
        t.hairline,
    );
}

/// The style of section captions: small, medium weight, tracked capitals.
#[must_use]
pub fn caption(p: &Painter<'_>, color: [f32; 4]) -> TextStyle {
    p.style(type_scale::CAPTION, color).medium().tracked(0.06)
}

/// A rounded pill with text, its left edge at `x` and centred on `cy`.
/// Returns the pill's rectangle.
pub fn pill(
    p: &mut Painter<'_>,
    x: f32,
    cy: f32,
    text: &str,
    style: &TextStyle,
    fill: [f32; 4],
) -> Rect {
    let pad = p.m.px(size::GRID * 2.0);
    let h = p.m.px(20.0);
    let w = (p.measure(text, style) + pad * 2.0).round();
    let rect = Rect::new(x.round(), (cy - h * 0.5).round(), w, h);
    p.fill_rounded(rect, fill, h * 0.5);
    p.text_in(text, rect, Align::Center, style);
    rect
}

/// Width a [`pill`] with `text` will take.
#[must_use]
pub fn pill_width(p: &Painter<'_>, text: &str, style: &TextStyle) -> f32 {
    (p.measure(text, style) + p.m.px(size::GRID * 2.0) * 2.0).round()
}

#[allow(clippy::many_single_char_names)] // p, t, x, y, w, h: painter, theme, geometry
pub fn tooltip(
    p: &mut Painter<'_>,
    anchor: Rect,
    bounds: Rect,
    label: &str,
    shortcut: Option<&str>,
) {
    let t = *p.theme;
    let pad_x = p.m.px(size::GRID * 2.0);
    let gap = p.m.px(size::GRID * 1.5);
    let h = p.m.px(24.0);
    let label_style = p.style(type_scale::BODY, t.text);
    let key_style = p.style(type_scale::CAPTION, t.text_muted);
    let label_w = p.measure(label, &label_style);
    let key_w = shortcut.map_or(0.0, |k| p.measure(k, &key_style) + pad_x * 1.5 + gap);
    let w = (label_w + key_w + pad_x * 2.0).round();
    let mut x = (anchor.center_x() - w * 0.5).round();
    x = x.clamp(
        bounds.x + gap,
        (bounds.right() - w - gap).max(bounds.x + gap),
    );
    let above = anchor.y - gap - h;
    let y = if above >= bounds.y {
        above
    } else {
        anchor.bottom() + gap
    };
    let rect = Rect::new(x, y.round(), w, h);
    p.shadow(
        rect,
        p.m.px(size::RADIUS_MD),
        p.m.px(4.0),
        p.m.px(2.0),
        t.shadow,
    );
    p.fill_rounded(rect, t.tooltip, p.m.px(size::RADIUS_MD));
    p.stroke_rounded(rect, t.border, p.m.px(size::RADIUS_MD), p.m.hairline());
    let inner = rect.inset_xy(pad_x, 0.0);
    p.text_in(label, inner, Align::Left, &label_style);
    if let Some(key) = shortcut {
        let kw = p.measure(key, &key_style) + pad_x;
        let key_rect = Rect::new(
            (inner.right() - kw).round(),
            (rect.center_y() - p.m.px(8.0)).round(),
            kw.round(),
            p.m.px(16.0),
        );
        p.fill_rounded(key_rect, t.control_hover, p.m.px(3.0));
        p.text_in(key, key_rect, Align::Center, &key_style);
    }
}

//! Render tools for agents (D-0043 phase 8).
//!
//! Everything here works on `ImageFrame`, a plain straight-alpha sRGB RGBA8
//! buffer. The tools never touch the user's document to draw an overlay:
//! marks, safe areas and grids are a **second pass** rendered from a synthetic
//! document and composited over the frame, so exported pixels cannot change.
//!
//! Pure image operations (resize, crop, paste, diff) need no GPU and are unit
//! tested; only the overlay pass and the textless background render touch the
//! engine.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)] // image pixels and sample indices; loop bounds are the buffer sizes

use std::collections::HashMap;

use motioner_core::{
    Color, Composition, Document, FrameRate, NodeContent, Paint, Property, PropertyValue, Stroke,
    TextAlign, TextContent, Vec2,
};
use motioner_eval::{Evaluator, RenderPlan};

use crate::{FrameRenderer, RenderError};

/// A straight-alpha sRGB image, row 0 at the top.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageFrame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba8: Vec<u8>,
}

impl ImageFrame {
    /// A transparent image.
    #[must_use]
    pub fn transparent(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba8: vec![0; width as usize * height as usize * 4],
        }
    }

    /// Whether the buffer matches the dimensions.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.rgba8.len() == self.width as usize * self.height as usize * 4
    }

    /// The pixel at `(x, y)` as straight RGBA.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let index = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.rgba8[index],
            self.rgba8[index + 1],
            self.rgba8[index + 2],
            self.rgba8[index + 3],
        ]
    }

    /// Scales the image so its longest edge is `long_edge` pixels.
    #[must_use]
    pub fn resized_to_long_edge(&self, long_edge: u32) -> Self {
        let longest = self.width.max(self.height).max(1);
        if long_edge == 0 || long_edge >= longest {
            return self.clone();
        }
        #[allow(clippy::cast_precision_loss)]
        let scale = f64::from(long_edge) / f64::from(longest);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let width = ((f64::from(self.width) * scale).round() as u32).max(1);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let height = ((f64::from(self.height) * scale).round() as u32).max(1);
        self.resized(width, height)
    }

    /// Box-filtered resize to exactly `width × height`.
    #[must_use]
    pub fn resized(&self, width: u32, height: u32) -> Self {
        if width == self.width && height == self.height {
            return self.clone();
        }
        if width == 0 || height == 0 || !self.is_valid() {
            return Self::transparent(width.max(1), height.max(1));
        }
        let mut out = vec![0_u8; width as usize * height as usize * 4];
        for y in 0..height {
            let y0 = (u64::from(y) * u64::from(self.height) / u64::from(height)) as u32;
            let y1 = (((u64::from(y) + 1) * u64::from(self.height)) / u64::from(height))
                .max(u64::from(y0) + 1)
                .min(u64::from(self.height)) as u32;
            for x in 0..width {
                let x0 = (u64::from(x) * u64::from(self.width) / u64::from(width)) as u32;
                let x1 = (((u64::from(x) + 1) * u64::from(self.width)) / u64::from(width))
                    .max(u64::from(x0) + 1)
                    .min(u64::from(self.width)) as u32;
                let mut total = [0_u64; 4];
                let mut count = 0_u64;
                for sy in y0..y1 {
                    for sx in x0..x1 {
                        let pixel = self.pixel(sx, sy);
                        for channel in 0..4 {
                            total[channel] += u64::from(pixel[channel]);
                        }
                        count += 1;
                    }
                }
                let index = (y as usize * width as usize + x as usize) * 4;
                for channel in 0..4 {
                    out[index + channel] =
                        u8::try_from(total[channel] / count.max(1)).unwrap_or(255);
                }
            }
        }
        Self {
            width,
            height,
            rgba8: out,
        }
    }

    /// Copies a rectangle into a new image.
    #[must_use]
    pub fn cropped(&self, rect: ImageRect) -> Self {
        let x0 = rect.x.max(0.0).min(f64::from(self.width));
        let y0 = rect.y.max(0.0).min(f64::from(self.height));
        let x1 = (rect.x + rect.width).max(x0).min(f64::from(self.width));
        let y1 = (rect.y + rect.height).max(y0).min(f64::from(self.height));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let width = (x1 - x0).round() as u32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let height = (y1 - y0).round() as u32;
        if width == 0 || height == 0 {
            return Self::transparent(1, 1);
        }
        let mut out = vec![0_u8; width as usize * height as usize * 4];
        for y in 0..height {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let source_y = (y0.round() as u32 + y).min(self.height - 1);
            for x in 0..width {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let source_x = (x0.round() as u32 + x).min(self.width - 1);
                let source = self.pixel(source_x, source_y);
                let index = (y as usize * width as usize + x as usize) * 4;
                out[index..index + 4].copy_from_slice(&source);
            }
        }
        Self {
            width,
            height,
            rgba8: out,
        }
    }

    /// Composites `overlay` over this image at `(x, y)`.
    pub fn composite(&mut self, overlay: &Self, x: u32, y: u32) {
        if !self.is_valid() || !overlay.is_valid() {
            return;
        }
        for oy in 0..overlay.height {
            let target_y = y.saturating_add(oy);
            if target_y >= self.height {
                break;
            }
            for ox in 0..overlay.width {
                let target_x = x.saturating_add(ox);
                if target_x >= self.width {
                    break;
                }
                let src = overlay.pixel(ox, oy);
                if src[3] == 0 {
                    continue;
                }
                let dst = self.pixel(target_x, target_y);
                let src_alpha = f64::from(src[3]) / 255.0;
                let dst_alpha = f64::from(dst[3]) / 255.0;
                let out_alpha = src_alpha + dst_alpha * (1.0 - src_alpha);
                if out_alpha <= 0.0 {
                    continue;
                }
                let blend = |source: u8, target: u8| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let value = ((f64::from(source) * src_alpha
                        + f64::from(target) * dst_alpha * (1.0 - src_alpha))
                        / out_alpha)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    value
                };
                let index = (target_y as usize * self.width as usize + target_x as usize) * 4;
                self.rgba8[index] = blend(src[0], dst[0]);
                self.rgba8[index + 1] = blend(src[1], dst[1]);
                self.rgba8[index + 2] = blend(src[2], dst[2]);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let alpha = (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
                self.rgba8[index + 3] = alpha;
            }
        }
    }

    /// Pastes `source` at `(x, y)` ignoring alpha (a tile composer).
    pub fn paste(&mut self, source: &Self, x: u32, y: u32) {
        self.composite(source, x, y);
    }

    /// The average straight colour of a rectangle, transparent pixels blended
    /// over `background`.
    #[must_use]
    pub fn average(&self, rect: ImageRect, background: Color) -> Option<Color> {
        if !self.is_valid() {
            return None;
        }
        let x0 = rect.x.max(0.0).min(f64::from(self.width));
        let y0 = rect.y.max(0.0).min(f64::from(self.height));
        let x1 = (rect.x + rect.width).max(x0).min(f64::from(self.width));
        let y1 = (rect.y + rect.height).max(y0).min(f64::from(self.height));
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let mut total = [0.0_f64; 3];
        let mut count = 0.0_f64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let start_x = x0.floor() as u32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let start_y = y0.floor() as u32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let end_x = x1.ceil().min(f64::from(self.width)) as u32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let end_y = y1.ceil().min(f64::from(self.height)) as u32;
        let background = [
            f64::from(background.r),
            f64::from(background.g),
            f64::from(background.b),
        ];
        for y in start_y..end_y {
            for x in start_x..end_x {
                let pixel = self.pixel(x.min(self.width - 1), y.min(self.height - 1));
                let alpha = f64::from(pixel[3]) / 255.0;
                for channel in 0..3 {
                    total[channel] +=
                        f64::from(pixel[channel]) * alpha + background[channel] * (1.0 - alpha);
                }
                count += 1.0;
            }
        }
        if count <= 0.0 {
            return None;
        }
        let value = |channel: usize| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let byte = (total[channel] / count).round().clamp(0.0, 255.0) as u8;
            byte
        };
        Some(Color::rgb(value(0), value(1), value(2)))
    }
}

/// A rectangle in image pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageRect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl ImageRect {
    /// A rectangle from its edges.
    #[must_use]
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width: width.max(0.0),
            height: height.max(0.0),
        }
    }

    /// The rectangle scaled by `factor`.
    #[must_use]
    pub fn scaled(self, factor: f64) -> Self {
        Self {
            x: self.x * factor,
            y: self.y * factor,
            width: self.width * factor,
            height: self.height * factor,
        }
    }

    /// The rectangle moved by `(dx, dy)`.
    #[must_use]
    pub fn translated(self, dx: f64, dy: f64) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }
}

/// One numbered overlay box.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayBox {
    /// Where the box is drawn, in output pixels.
    pub rect: ImageRect,
    /// Number the legend uses (`#1`, `#2`, ...).
    pub number: usize,
    /// Text drawn next to the box.
    pub label: String,
    /// Box and label colour.
    pub color: Color,
}

/// What the overlay pass draws over a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    /// Nothing.
    None,
    /// Numbered boxes and a legend (`#n → id, name`).
    Marks,
    /// Title- and action-safe rectangles.
    Safe,
    /// Thirds grid plus safe rectangles.
    Grid,
    /// Bounds only, without numbers.
    Bounds,
}

impl Overlay {
    /// Stable lower-case name used in tool results and cache keys.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Marks => "marks",
            Self::Safe => "safe",
            Self::Grid => "grid",
            Self::Bounds => "bounds",
        }
    }
}

/// One free text drawn by the overlay pass.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayCaption {
    /// Baseline position in output pixels.
    pub position: (f64, f64),
    /// Text.
    pub text: String,
    /// Glyph size.
    pub size: f64,
    /// Colour.
    pub color: Color,
}

/// The boxes and legend of one overlay pass, in output pixels.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OverlaySpec {
    /// Rectangles to draw.
    pub boxes: Vec<OverlayBox>,
    /// Legend lines, drawn bottom-left.
    pub legend: Vec<String>,
    /// Free captions (frame labels of a strip or sheet).
    pub captions: Vec<OverlayCaption>,
    /// Draw the safe rectangles.
    pub safe: bool,
    /// Draw the thirds grid.
    pub grid: bool,
}

impl OverlaySpec {
    /// Scales every rectangle and every legend position by `factor`
    /// (composition pixels → output pixels).
    #[must_use]
    pub fn scaled(&self, factor: f64) -> Self {
        Self {
            boxes: self
                .boxes
                .iter()
                .map(|box_| OverlayBox {
                    rect: box_.rect.scaled(factor),
                    number: box_.number,
                    label: box_.label.clone(),
                    color: box_.color,
                })
                .collect(),
            legend: self.legend.clone(),
            captions: self
                .captions
                .iter()
                .map(|caption| OverlayCaption {
                    position: (caption.position.0 * factor, caption.position.1 * factor),
                    text: caption.text.clone(),
                    size: caption.size * factor,
                    color: caption.color,
                })
                .collect(),
            safe: self.safe,
            grid: self.grid,
        }
    }

    /// Whether the overlay draws nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
            && self.legend.is_empty()
            && self.captions.is_empty()
            && !self.safe
            && !self.grid
    }
}

fn add_rect(
    document: &mut Document,
    rect: ImageRect,
    fill: Option<Color>,
    stroke: Option<(Color, f64)>,
) -> Result<(), motioner_core::CoreError> {
    let paint = Paint {
        fill,
        stroke: stroke.map(|(color, width)| Stroke { color, width }),
    };
    let id = document.add_node(NodeContent::rect(rect.width, rect.height, 0.0, paint))?;
    document.set_property(
        id,
        Property::Position,
        PropertyValue::Vec2(Vec2::new(
            rect.x + rect.width * 0.5,
            rect.y + rect.height * 0.5,
        )),
    )
}

fn add_text(
    document: &mut Document,
    position: (f64, f64),
    text: &str,
    size: f64,
    color: Color,
) -> Result<(), motioner_core::CoreError> {
    let content = TextContent::new(text, "Roboto", size, color, TextAlign::Left);
    let id = document.add_node(NodeContent::Text(content))?;
    document.set_property(
        id,
        Property::Position,
        PropertyValue::Vec2(Vec2::new(position.0, position.1)),
    )
}

/// Draws an overlay into a transparent image with its own synthetic document.
///
/// This is a separate GPU pass; nothing here is added to the user's document.
///
/// # Errors
///
/// Returns [`RenderError`] when the engine cannot create the overlay renderer
/// or render it.
#[allow(clippy::too_many_lines)] // one pass per overlay element, kept linear
pub fn render_overlay(
    width: u32,
    height: u32,
    spec: &OverlaySpec,
    safe_margins: (f64, f64),
) -> Result<ImageFrame, RenderError> {
    let composition = Composition::new(
        "overlay",
        width.max(1),
        height.max(1),
        FrameRate::new(60, 1).map_err(|error| RenderError::Image {
            message: error.to_string(),
        })?,
        1,
        Color::rgba(0, 0, 0, 0),
    )
    .map_err(|error| RenderError::Image {
        message: error.to_string(),
    })?;
    let mut document = Document::new(composition);
    let map = |error: motioner_core::CoreError| RenderError::Image {
        message: error.to_string(),
    };
    for box_ in &spec.boxes {
        add_rect(&mut document, box_.rect, None, Some((box_.color, 2.0))).map_err(map)?;
        if !spec.legend.is_empty() {
            add_text(
                &mut document,
                (box_.rect.x + 4.0, box_.rect.y - 6.0),
                &format!("#{}", box_.number),
                12.0,
                box_.color,
            )
            .map_err(map)?;
        }
    }
    for caption in &spec.captions {
        add_text(
            &mut document,
            caption.position,
            &caption.text,
            caption.size,
            caption.color,
        )
        .map_err(map)?;
    }
    if spec.grid {
        let third_w = f64::from(width) / 3.0;
        let third_h = f64::from(height) / 3.0;
        for index in 1..3 {
            #[allow(clippy::cast_precision_loss)]
            let x = third_w * index as f64;
            add_rect(
                &mut document,
                ImageRect::new(x, 0.0, 1.0, f64::from(height)),
                Some(Color::rgba(255, 255, 255, 60)),
                None,
            )
            .map_err(map)?;
            #[allow(clippy::cast_precision_loss)]
            let y = third_h * index as f64;
            add_rect(
                &mut document,
                ImageRect::new(0.0, y, f64::from(width), 1.0),
                Some(Color::rgba(255, 255, 255, 60)),
                None,
            )
            .map_err(map)?;
        }
    }
    if spec.safe {
        let (title, action) = (
            ImageRect::new(
                f64::from(width) * safe_margins.0,
                f64::from(height) * safe_margins.0,
                f64::from(width) * (1.0 - safe_margins.0 * 2.0),
                f64::from(height) * (1.0 - safe_margins.0 * 2.0),
            ),
            ImageRect::new(
                f64::from(width) * safe_margins.1,
                f64::from(height) * safe_margins.1,
                f64::from(width) * (1.0 - safe_margins.1 * 2.0),
                f64::from(height) * (1.0 - safe_margins.1 * 2.0),
            ),
        );
        add_rect(
            &mut document,
            title,
            None,
            Some((Color::rgba(255, 200, 80, 220), 1.5)),
        )
        .map_err(map)?;
        add_rect(
            &mut document,
            action,
            None,
            Some((Color::rgba(120, 200, 255, 160), 1.0)),
        )
        .map_err(map)?;
    }
    if !spec.legend.is_empty() {
        let line_height = 16.0;
        #[allow(clippy::cast_precision_loss)]
        let base_y = f64::from(height) - 8.0 - line_height * spec.legend.len() as f64;
        let plate_height = line_height * spec.legend.len() as f64 + 8.0;
        add_rect(
            &mut document,
            ImageRect::new(8.0, base_y, 380.0, plate_height),
            Some(Color::rgba(0, 0, 0, 190)),
            None,
        )
        .map_err(map)?;
        for (index, line) in spec.legend.iter().enumerate() {
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let y = base_y + 12.0 + line_height * index as f64;
            add_text(
                &mut document,
                (14.0, y),
                line,
                12.0,
                Color::rgb(235, 240, 255),
            )
            .map_err(map)?;
        }
    }
    let mut renderer = FrameRenderer::new(width.max(1), height.max(1))?;
    let mut rgba = Vec::new();
    renderer.render_frame(&document, 0, &mut rgba)?;
    Ok(ImageFrame {
        width: width.max(1),
        height: height.max(1),
        rgba8: rgba,
    })
}

/// Composes frames into one horizontal strip, each cell scaled to the same
/// height and separated by `gap` pixels of transparent space.
#[must_use]
pub fn compose_strip(frames: &[ImageFrame], cell_width: u32, gap: u32) -> ImageFrame {
    if frames.is_empty() {
        return ImageFrame::transparent(1, 1);
    }
    let cell_height = frames
        .iter()
        .map(|frame| {
            if frame.width == 0 {
                1
            } else {
                (u64::from(frame.height) * u64::from(cell_width) / u64::from(frame.width)).max(1)
                    as u32
            }
        })
        .max()
        .unwrap_or(1);
    let count = frames.len() as u32;
    let width = count * cell_width + count.saturating_sub(1) * gap;
    let mut out = ImageFrame::transparent(width, cell_height);
    let mut x = 0;
    for frame in frames {
        let cell = frame.resized(cell_width.max(1), cell_height.max(1));
        let y = (cell_height.saturating_sub(cell.height)) / 2;
        out.composite(&cell, x.min(width.saturating_sub(1)), y);
        x = x.saturating_add(cell_width + gap);
    }
    out
}

/// Composes frames into a grid with `columns` cells per row.
#[must_use]
pub fn compose_sheet(frames: &[ImageFrame], columns: u32, cell_width: u32, gap: u32) -> ImageFrame {
    if frames.is_empty() {
        return ImageFrame::transparent(1, 1);
    }
    let columns = columns.max(1);
    let cell_height = frames
        .iter()
        .map(|frame| {
            if frame.width == 0 {
                1
            } else {
                (u64::from(frame.height) * u64::from(cell_width) / u64::from(frame.width)).max(1)
                    as u32
            }
        })
        .max()
        .unwrap_or(1);
    let rows = (frames.len() as u32).div_ceil(columns);
    let width = columns * cell_width + columns.saturating_sub(1) * gap;
    let height = rows * cell_height + rows.saturating_sub(1) * gap;
    let mut out = ImageFrame::transparent(width, height);
    for (index, frame) in frames.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let index = index as u32;
        let column = index % columns;
        let row = index / columns;
        let cell = frame.resized(cell_width.max(1), cell_height.max(1));
        out.composite(
            &cell,
            column * (cell_width + gap),
            row * (cell_height + gap),
        );
    }
    out
}

/// How `compare` renders the two frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareMode {
    /// Left and right, separated by a white line.
    Side,
    /// A diff heatmap plus the changed regions.
    Diff,
}

impl CompareMode {
    /// Stable lower-case name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Side => "side",
            Self::Diff => "diff",
        }
    }
}

/// The result of a diff: the heatmap and the regions that changed.
#[derive(Debug, Clone, PartialEq)]
pub struct CompareResult {
    /// The image to show.
    pub image: ImageFrame,
    /// Changed regions, in pixels of the compared images.
    pub changed: Vec<ImageRect>,
    /// Mean absolute change across the images, `0.0..=1.0`.
    pub mean_change: f64,
}

/// Compares two same-size images.
#[must_use]
pub fn compare(a: &ImageFrame, b: &ImageFrame, mode: CompareMode) -> CompareResult {
    let (width, height) = (a.width.max(b.width), a.height.max(b.height));
    let a = a.resized(width, height);
    let b = b.resized(width, height);
    match mode {
        CompareMode::Side => {
            let mut image = ImageFrame::transparent(width * 2 + 1, height);
            image.composite(&a, 0, 0);
            image.composite(&b, width + 1, 0);
            let divider = ImageFrame::transparent(1, height);
            image.composite(&divider, width, 0);
            CompareResult {
                image,
                changed: diff_regions(&a, &b, 16, 0.06),
                mean_change: mean_difference(&a, &b),
            }
        }
        CompareMode::Diff => {
            let mut image = ImageFrame::transparent(width, height);
            let mut total = 0.0_f64;
            for y in 0..height {
                for x in 0..width {
                    let left = a.pixel(x.min(a.width - 1), y.min(a.height - 1));
                    let right = b.pixel(x.min(b.width - 1), y.min(b.height - 1));
                    let mut difference = 0.0_f64;
                    for channel in 0..3 {
                        difference += f64::from(left[channel].abs_diff(right[channel]));
                    }
                    difference /= 3.0 * 255.0;
                    total += difference;
                    // Heatmap: black when equal, yellow to red when changed.
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let intensity = (difference * 3.0).clamp(0.0, 1.0);
                    let red = (intensity * 255.0) as u8;
                    let green = (intensity * 180.0) as u8;
                    let blue = 0_u8;
                    let index = (y as usize * width as usize + x as usize) * 4;
                    image.rgba8[index] = red;
                    image.rgba8[index + 1] = green;
                    image.rgba8[index + 2] = blue;
                    image.rgba8[index + 3] = 255;
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let mean_change = total / (f64::from(width) * f64::from(height)).max(1.0);
            CompareResult {
                changed: diff_regions(&a, &b, 16, 0.06),
                image,
                mean_change,
            }
        }
    }
}

fn mean_difference(a: &ImageFrame, b: &ImageFrame) -> f64 {
    if !a.is_valid() || !b.is_valid() {
        return 0.0;
    }
    let mut total = 0.0_f64;
    let mut count = 0.0_f64;
    for y in 0..a.height.min(b.height) {
        for x in 0..a.width.min(b.width) {
            let left = a.pixel(x, y);
            let right = b.pixel(x, y);
            for channel in 0..3 {
                total += f64::from(left[channel].abs_diff(right[channel])) / 255.0;
            }
            count += 3.0;
        }
    }
    if count <= 0.0 {
        0.0
    } else {
        total / count
    }
}

/// Bounding boxes of a `grid × grid` cell map whose mean difference is above
/// `threshold`, merged into rectangles.
#[must_use]
pub fn diff_regions(a: &ImageFrame, b: &ImageFrame, grid: u32, threshold: f64) -> Vec<ImageRect> {
    let grid = grid.max(1);
    let width = a.width.min(b.width);
    let height = a.height.min(b.height);
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let mut changed = vec![vec![false; grid as usize]; grid as usize];
    for row in 0..grid {
        for col in 0..grid {
            let x0 = col * width / grid;
            let x1 = ((col + 1) * width / grid).max(x0 + 1).min(width);
            let y0 = row * height / grid;
            let y1 = ((row + 1) * height / grid).max(y0 + 1).min(height);
            let mut total = 0.0_f64;
            let mut count = 0.0_f64;
            for y in y0..y1 {
                for x in x0..x1 {
                    let left = a.pixel(x, y);
                    let right = b.pixel(x, y);
                    for channel in 0..3 {
                        total += f64::from(left[channel].abs_diff(right[channel])) / 255.0;
                    }
                    count += 3.0;
                }
            }
            if count > 0.0 && total / count > threshold {
                changed[row as usize][col as usize] = true;
            }
        }
    }
    // Merge changed cells into axis-aligned runs per row, then merge identical
    // runs across adjacent rows.
    let mut regions: Vec<(u32, u32, u32, u32)> = Vec::new(); // col0,row0,col1,row1 inclusive
    for row in 0..grid {
        let mut col = 0;
        while col < grid {
            if !changed[row as usize][col as usize] {
                col += 1;
                continue;
            }
            let start = col;
            while col < grid && changed[row as usize][col as usize] {
                col += 1;
            }
            let end = col - 1;
            if let Some(region) = regions
                .iter_mut()
                .find(|region| region.1 + 1 == row && region.0 == start && region.2 == end)
            {
                region.3 = row;
            } else {
                regions.push((start, row, end, row));
            }
        }
    }
    regions
        .into_iter()
        .map(|(col0, row0, col1, row1)| {
            ImageRect::new(
                f64::from(col0 * width / grid),
                f64::from(row0 * height / grid),
                f64::from((col1 + 1) * width / grid - col0 * width / grid),
                f64::from((row1 + 1) * height / grid - row0 * height / grid),
            )
        })
        .collect()
}

/// A frame rendered with every text node hidden, for contrast sampling.
#[derive(Debug, Clone, PartialEq)]
pub struct TextlessFrame {
    /// Frame index.
    pub frame: i64,
    /// Composition size.
    pub width: u32,
    /// Composition size.
    pub height: u32,
    /// The rendered pixels.
    pub image: ImageFrame,
    /// The composition background, used for transparent pixels.
    pub background: Color,
}

impl TextlessFrame {
    /// Renders `frame` with every text node hidden.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] from the renderer.
    pub fn render(
        renderer: &mut FrameRenderer,
        document: &Document,
        frame: i64,
    ) -> Result<Self, RenderError> {
        let mut copy = document.clone();
        let text_nodes: Vec<_> = copy
            .nodes()
            .filter(|node| matches!(node.content(), NodeContent::Text(_)))
            .map(motioner_core::Node::id)
            .collect();
        for id in text_nodes {
            let _ = copy.set_visible(id, false);
        }
        let mut rgba = Vec::new();
        renderer.render_frame(&copy, frame, &mut rgba)?;
        let composition = document.composition();
        Ok(Self {
            frame,
            width: composition.width(),
            height: composition.height(),
            image: ImageFrame {
                width: composition.width(),
                height: composition.height(),
                rgba8: rgba,
            },
            background: composition.background(),
        })
    }

    /// The average colour behind a rectangle (composition pixels).
    #[must_use]
    pub fn behind(&self, rect: ImageRect) -> Option<Color> {
        let _ = self.frame;
        self.image.average(rect, self.background)
    }
}

/// An in-memory cache of rendered images (D-0043 §3).
///
/// The key is exactly the tuple the ADR names: program hash, frame, size and
/// overlay. The cache is bounded; inserting past the limit drops the oldest
/// entry (insertion order), so a long agent session cannot grow without end.
#[derive(Debug, Default)]
pub struct ImageCache {
    entries: HashMap<CacheKey, ImageFrame>,
    order: Vec<CacheKey>,
    limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    program: String,
    frame: i64,
    size: u32,
    overlay: &'static str,
}

impl ImageCache {
    /// A cache holding at most `limit` images.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// The cached image for the key, if present.
    #[must_use]
    pub fn get(
        &self,
        program: &str,
        frame: i64,
        size: u32,
        overlay: Overlay,
    ) -> Option<&ImageFrame> {
        let key = CacheKey {
            program: program.to_owned(),
            frame,
            size,
            overlay: overlay.name(),
        };
        self.entries.get(&key)
    }

    /// Stores an image, evicting the oldest entries past the limit.
    pub fn insert(
        &mut self,
        program: &str,
        frame: i64,
        size: u32,
        overlay: Overlay,
        image: ImageFrame,
    ) {
        let key = CacheKey {
            program: program.to_owned(),
            frame,
            size,
            overlay: overlay.name(),
        };
        if self.entries.insert(key.clone(), image).is_none() {
            self.order.push(key);
        }
        while self.order.len() > self.limit {
            let oldest = self.order.remove(0);
            self.entries.remove(&oldest);
        }
    }

    /// Number of cached images.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Renders `frame` at the composition size.
///
/// # Errors
///
/// Returns [`RenderError`] from the renderer.
pub fn render_full(
    renderer: &mut FrameRenderer,
    document: &Document,
    frame: i64,
) -> Result<ImageFrame, RenderError> {
    let mut rgba = Vec::new();
    renderer.render_frame(document, frame, &mut rgba)?;
    let composition = document.composition();
    Ok(ImageFrame {
        width: composition.width(),
        height: composition.height(),
        rgba8: rgba,
    })
}

/// Evaluates a frame into a plan (used by the callers that need boxes).
#[must_use]
pub fn frame_plan(document: &Document, frame: i64) -> RenderPlan {
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    evaluator.evaluate(document, frame, &mut plan);
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, color: [u8; 4]) -> ImageFrame {
        ImageFrame {
            width,
            height,
            rgba8: color
                .iter()
                .copied()
                .cycle()
                .take(width as usize * height as usize * 4)
                .collect(),
        }
    }

    #[test]
    fn resize_preserves_a_solid_colour() {
        let solid = image(10, 10, [10, 20, 30, 255]);
        let small = solid.resized(5, 4);
        assert_eq!(small.width, 5);
        assert_eq!(small.height, 4);
        assert_eq!(small.pixel(0, 0), [10, 20, 30, 255]);
        assert_eq!(small.pixel(4, 3), [10, 20, 30, 255]);
    }

    #[test]
    fn crop_keeps_the_requested_region() {
        let solid = image(20, 10, [1, 2, 3, 255]);
        let cropped = solid.cropped(ImageRect::new(5.0, 2.0, 4.0, 3.0));
        assert_eq!((cropped.width, cropped.height), (4, 3));
        assert_eq!(cropped.pixel(1, 1), [1, 2, 3, 255]);
    }

    #[test]
    fn composite_alpha_blends_straight_over() {
        let mut base = image(2, 2, [0, 0, 0, 255]);
        let overlay = image(1, 1, [255, 255, 255, 128]);
        base.composite(&overlay, 0, 0);
        let pixel = base.pixel(0, 0);
        assert!((120..=136).contains(&pixel[0]), "{pixel:?}");
        assert_eq!(pixel[3], 255);
    }

    #[test]
    fn strip_and_sheet_compose_every_frame() {
        let frames = [image(4, 4, [10, 0, 0, 255]), image(4, 4, [0, 10, 0, 255])];
        let strip = compose_strip(&frames, 4, 2);
        assert_eq!(strip.width, 10);
        assert_eq!(strip.height, 4);
        let sheet = compose_sheet(&frames, 2, 4, 2);
        assert_eq!(sheet.width, 10);
        assert_eq!(sheet.height, 4);
    }

    #[test]
    fn diff_regions_land_on_the_changed_half() {
        let left = image(40, 20, [0, 0, 0, 255]);
        let mut right = left.clone();
        for y in 0..20 {
            for x in 20..40 {
                let index = (y as usize * 40 + x as usize) * 4;
                right.rgba8[index] = 255;
            }
        }
        let regions = diff_regions(&left, &right, 8, 0.05);
        assert!(!regions.is_empty());
        for region in &regions {
            assert!(region.x >= 19.0, "{region:?}");
        }
        let comparison = compare(&left, &right, CompareMode::Diff);
        assert!(comparison.mean_change > 0.0);
        assert_eq!(comparison.image.width, 40);
    }

    #[test]
    fn the_cache_is_bounded_and_keyed_by_the_contract() {
        let mut cache = ImageCache::new(2);
        cache.insert("p", 1, 768, Overlay::None, image(1, 1, [1, 1, 1, 255]));
        assert!(cache.get("p", 1, 768, Overlay::None).is_some());
        assert!(cache.get("p", 1, 512, Overlay::None).is_none());
        assert!(cache.get("q", 1, 768, Overlay::None).is_none());
        cache.insert("p", 2, 768, Overlay::None, image(1, 1, [1, 1, 1, 255]));
        cache.insert("p", 3, 768, Overlay::None, image(1, 1, [1, 1, 1, 255]));
        assert_eq!(cache.len(), 2);
        assert!(cache.get("p", 1, 768, Overlay::None).is_none());
    }
}

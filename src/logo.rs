use eframe::egui;
use usvg::TreeParsing;

/// Convert an SVG document (as a string) into an `egui::ColorImage` by
/// rendering it via `resvg`/`usvg`/`tiny_skia` and decoding the result with
/// the `image` crate. This is purposely standalone so the caller can cache the
/// texture handle in the UI context and avoid re-parsing on every frame.

pub fn color_image_from_svg(svg_data: &str) -> Option<egui::ColorImage> {
    // usvg will parse the SVG and compute an absolute size.  We then render it
    // into a tiny-skia pixmap and convert that to an `egui::ColorImage` using
    // the image crate to normalise pixel format.
    let opt = usvg::Options::default();
    let rtree = usvg::Tree::from_data(svg_data.as_bytes(), &opt).ok()?;

    // Compute the dimension we will rasterize at.
    let size = rtree.size.to_int_size();
    let width = size.width().max(1);
    let height = size.height().max(1);

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    let mut render_rtree = resvg::Tree::from_usvg(&rtree);
    render_rtree.render(usvg::Transform::default(), &mut pixmap.as_mut());

    // Encode to PNG and then re-decode with `image` so we can easily obtain
    // RGBA8 unpremultiplied bytes that egui understands.  This avoids having to
    // reimplement premultiplied->unpremultiplied conversion ourselves.
    let png = pixmap.encode_png().ok()?;
    let dyn_img = image::load_from_memory(&png).ok()?.to_rgba8();
    let (w, h) = dyn_img.dimensions();
    let pixels = dyn_img.into_raw();

    Some(egui::ColorImage::from_rgba_unmultiplied([
        w as usize,
        h as usize,
    ],
    &pixels))
}

/// Convert an SVG document into an `egui::IconData` which can be used for the app window.
pub fn icon_data_from_svg(svg_data: &str) -> Option<egui::IconData> {
    let opt = usvg::Options::default();
    let rtree = usvg::Tree::from_data(svg_data.as_bytes(), &opt).ok()?;

    let size = rtree.size.to_int_size();
    let width = size.width().max(1);
    let height = size.height().max(1);

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    let mut render_rtree = resvg::Tree::from_usvg(&rtree);
    render_rtree.render(usvg::Transform::default(), &mut pixmap.as_mut());

    let png = pixmap.encode_png().ok()?;
    let dyn_img = image::load_from_memory(&png).ok()?.to_rgba8();
    let (w, h) = dyn_img.dimensions();
    let pixels = dyn_img.into_raw();

    Some(egui::IconData {
        rgba: pixels,
        width: w,
        height: h,
    })
}

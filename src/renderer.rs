// This module previously contained a CPU rasteriser/ffmpeg exporter used by
// the old export pipeline.  That functionality has been removed; the file is
// kept empty so downstream crates (if any) continue to compile without
// churn.  The sole function `render_and_encode` was unused and has been
// deleted to satisfy `#![deny(dead_code)]`.

//! Biblioteca da mira (raster tiny-skia) — compartilhada com a GUI.

pub mod render;

pub use render::{center_alpha, count_opaque, pixmap_size, render_crosshair};

//! Icon glyphs (Phosphor icon font).

pub use egui_phosphor::regular::{
    ARROW_CLOCKWISE as ROTATE_CW, ARROW_COUNTER_CLOCKWISE as ROTATE_CCW, ARROW_DOWN, ARROW_LEFT, ARROW_RIGHT, ARROW_U_UP_LEFT as UNDO,
    ARROW_U_UP_RIGHT as REDO, ARROW_UP, ARROWS_CLOCKWISE as REFRESH, CARET_DOWN, CARET_RIGHT, COPY, DICE_FIVE as DICE,
    DOWNLOAD_SIMPLE as EXPORT, FIRE, FLIP_HORIZONTAL, FLIP_VERTICAL, FRAME_CORNERS as FIT, MINUS, PLUS, SPARKLE, SWAP, TRIANGLE, X,
};

pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

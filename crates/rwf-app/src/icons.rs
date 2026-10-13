// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
//
// Ported from JWildfire - an image and animation processor written in Java
// Copyright (C) 1995-2026 Andreas Maschke
//
// This program is free software: you can redistribute it and/or modify it under the terms of
// the GNU Lesser General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License along with this
// program. If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Icon glyphs (Phosphor icon font).

pub use egui_phosphor::regular::{
    ARROW_CLOCKWISE as ROTATE_CW, ARROW_COUNTER_CLOCKWISE as ROTATE_CCW, ARROW_DOWN, ARROW_LEFT, ARROW_RIGHT, ARROW_U_UP_LEFT as UNDO,
    ARROW_U_UP_RIGHT as REDO, ARROW_UP, ARROWS_CLOCKWISE as REFRESH, CARET_DOWN, CARET_RIGHT, COPY, DICE_FIVE as DICE,
    DOWNLOAD_SIMPLE as EXPORT, EYE, EYE_SLASH, FIRE, FLIP_HORIZONTAL, FLIP_VERTICAL, FRAME_CORNERS as FIT, MINUS, PALETTE, PLUS, SKIP_BACK, SKIP_FORWARD, PLAY, PAUSE, RECORD, DIAMOND, FILM_STRIP, SPARKLE, SWAP, TRIANGLE, X,
};

pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

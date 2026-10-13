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

//! The edited document: flame, selection and undo history.

use std::path::PathBuf;

use rwf_core::model::XFormKind;
use rwf_core::{Flame, Layer, XForm};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    None,
    XForm(usize),
    Final(usize),
    Background(usize),
}

impl Selection {
    /// The xform list and index of the selection.
    pub fn kind(self) -> Option<(XFormKind, usize)> {
        match self {
            Selection::XForm(i) => Some((XFormKind::Normal, i)),
            Selection::Final(i) => Some((XFormKind::Final, i)),
            Selection::Background(i) => Some((XFormKind::Background, i)),
            Selection::None => None,
        }
    }

    pub fn of(kind: XFormKind, i: usize) -> Self {
        match kind {
            XFormKind::Normal => Selection::XForm(i),
            XFormKind::Final => Selection::Final(i),
            XFormKind::Background => Selection::Background(i),
        }
    }
}

pub struct Document {
    pub flame: Flame,
    pub path: Option<PathBuf>,
    /// All flames of the last opened file (for multi-flame files).
    pub library: Vec<Flame>,
    pub layer: usize,
    pub selection: Selection,
    /// Edit post-affine instead of pre-affine.
    pub edit_post: bool,
    undo: Vec<Flame>,
    redo: Vec<Flame>,
    committed: Flame,
    pub dirty: bool,
}

const MAX_UNDO: usize = 200;

impl Document {
    pub fn new(flame: Flame) -> Self {
        let selection = if flame.first_layer().xforms.is_empty() { Selection::None } else { Selection::XForm(0) };
        Self {
            committed: flame.clone(),
            flame,
            path: None,
            library: Vec::new(),
            layer: 0,
            selection,
            edit_post: false,
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
        }
    }

    pub fn layer(&self) -> &Layer {
        &self.flame.layers[self.layer.min(self.flame.layers.len() - 1)]
    }

    pub fn layer_mut(&mut self) -> &mut Layer {
        let i = self.layer.min(self.flame.layers.len() - 1);
        &mut self.flame.layers[i]
    }


    pub fn selected_mut(&mut self) -> Option<&mut XForm> {
        let (kind, i) = self.selection.kind()?;
        self.layer_mut().xforms_of_mut(kind).get_mut(i)
    }

    /// Makes sure the selection points to an existing transform.
    pub fn fix_selection(&mut self) {
        self.layer = self.layer.min(self.flame.layers.len().saturating_sub(1));
        let l = self.layer();
        let (nx, nf, nb) = (l.xforms.len(), l.final_xforms.len(), l.bg_xforms.len());
        self.selection = match self.selection {
            Selection::XForm(i) if i < nx => Selection::XForm(i),
            Selection::Final(i) if i < nf => Selection::Final(i),
            Selection::Background(i) if i < nb => Selection::Background(i),
            _ if nx > 0 => Selection::XForm(nx - 1),
            _ => Selection::None,
        };
    }

    /// Records an undo step once an edit has settled (no pointer button held).
    pub fn commit_if_settled(&mut self, pointer_down: bool) {
        if pointer_down || self.flame == self.committed {
            return;
        }
        let prev = std::mem::replace(&mut self.committed, self.flame.clone());
        self.undo.push(prev);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    /// The flame state before the pending (not yet committed) edit.
    pub fn committed(&self) -> &Flame {
        &self.committed
    }

    /// Evaluates the animation at `frame` without creating an undo step.
    pub fn show_frame(&mut self, frame: f64) {
        self.commit_if_settled(false);
        self.flame.apply_animation(frame);
        self.committed = self.flame.clone();
    }

    /// Replaces the whole flame as one undoable step.
    pub fn replace(&mut self, flame: Flame) {
        self.commit_if_settled(false);
        self.undo.push(std::mem::replace(&mut self.flame, flame));
        self.committed = self.flame.clone();
        self.redo.clear();
        self.dirty = true;
        self.fix_selection();
    }

    /// Loads a new document (clears history).
    pub fn load(&mut self, flame: Flame, path: Option<PathBuf>) {
        self.replace(flame);
        self.path = path;
        self.layer = 0;
        self.selection = Selection::XForm(0);
        self.fix_selection();
        self.dirty = false;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        self.commit_if_settled(false);
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.flame, prev));
            self.committed = self.flame.clone();
            self.fix_selection();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.flame, next));
            self.committed = self.flame.clone();
            self.fix_selection();
        }
    }

    pub fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| if self.flame.name.is_empty() { "Untitled".into() } else { self.flame.name.clone() });
        if self.dirty { format!("{name} •") } else { name }
    }
}

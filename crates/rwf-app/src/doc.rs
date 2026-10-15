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
use rwf_core::{Affine, Flame, Layer, XForm};

/// The affine plane being edited (JWildfire's `EditPlane`): XY, or the YZ / ZX affines of 3D
/// transforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EditPlane {
    #[default]
    XY,
    YZ,
    ZX,
}

impl EditPlane {
    pub const ALL: [EditPlane; 3] = [EditPlane::XY, EditPlane::YZ, EditPlane::ZX];

    pub fn label(&self) -> &'static str {
        match self {
            EditPlane::XY => "XY",
            EditPlane::YZ => "YZ",
            EditPlane::ZX => "ZX",
        }
    }
}

/// The affine of `xf` edited in `plane` (pre or post).
pub fn edit_affine(xf: &XForm, plane: EditPlane, post: bool) -> &Affine {
    match (plane, post) {
        (EditPlane::XY, false) => &xf.coefs,
        (EditPlane::XY, true) => &xf.post,
        (EditPlane::YZ, false) => &xf.yz_coefs,
        (EditPlane::YZ, true) => &xf.yz_post,
        (EditPlane::ZX, false) => &xf.zx_coefs,
        (EditPlane::ZX, true) => &xf.zx_post,
    }
}

pub fn edit_affine_mut(xf: &mut XForm, plane: EditPlane, post: bool) -> &mut Affine {
    match (plane, post) {
        (EditPlane::XY, false) => &mut xf.coefs,
        (EditPlane::XY, true) => &mut xf.post,
        (EditPlane::YZ, false) => &mut xf.yz_coefs,
        (EditPlane::YZ, true) => &mut xf.yz_post,
        (EditPlane::ZX, false) => &mut xf.zx_coefs,
        (EditPlane::ZX, true) => &mut xf.zx_post,
    }
}

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
    /// The file `library` was read from (`None` for pasted or script-made flames).
    pub library_path: Option<PathBuf>,
    /// The file and flame index the editor's flame was loaded from or last saved to; saving into
    /// that file replaces that flame and keeps the others. Undo and redo restore it.
    pub source: Option<(PathBuf, usize)>,
    pub layer: usize,
    pub selection: Selection,
    /// Edit post-affine instead of pre-affine.
    pub edit_post: bool,
    pub edit_plane: EditPlane,
    undo: Vec<(Flame, Option<(PathBuf, usize)>)>,
    redo: Vec<(Flame, Option<(PathBuf, usize)>)>,
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
            library_path: None,
            source: None,
            layer: 0,
            selection,
            edit_post: false,
            edit_plane: EditPlane::XY,
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
        self.undo.push((prev, self.source.clone()));
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

    /// Replaces the whole flame as one undoable step. The new flame comes from no file; set
    /// `source` afterwards if it does.
    pub fn replace(&mut self, flame: Flame) {
        self.commit_if_settled(false);
        let prev = std::mem::replace(&mut self.flame, flame);
        self.undo.push((prev, self.source.take()));
        self.committed = self.flame.clone();
        self.redo.clear();
        self.dirty = true;
        self.fix_selection();
    }

    /// Loads a new document (clears history).
    pub fn load(&mut self, flame: Flame, path: Option<PathBuf>) {
        self.replace(flame);
        self.source = path.clone().map(|p| (p, 0));
        self.path = path;
        self.layer = 0;
        self.selection = Selection::XForm(0);
        self.fix_selection();
        self.dirty = false;
    }

    /// The flames to write when saving the editor's flame to `path`, which currently holds
    /// `existing` (empty if it does not exist or cannot be read), and the editor flame's index
    /// among them. A file with several flames keeps them: the editor's flame replaces the flame
    /// it came from, or is appended if it is not from this file. Other files get the editor's
    /// flame alone. Sets `source` to the written flame.
    pub fn flames_for_save(&mut self, path: &std::path::Path, mut existing: Vec<Flame>) -> (Vec<Flame>, usize) {
        let index = if existing.len() > 1 {
            match self.source.as_ref().filter(|(p, i)| p == path && *i < existing.len()) {
                Some((_, i)) => {
                    existing[*i] = self.flame.clone();
                    *i
                }
                None => {
                    existing.push(self.flame.clone());
                    existing.len() - 1
                }
            }
        } else {
            existing = vec![self.flame.clone()];
            0
        };
        self.source = Some((path.to_path_buf(), index));
        (existing, index)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        self.commit_if_settled(false);
        if let Some((prev, source)) = self.undo.pop() {
            let cur = std::mem::replace(&mut self.flame, prev);
            self.redo.push((cur, std::mem::replace(&mut self.source, source)));
            self.committed = self.flame.clone();
            self.fix_selection();
        }
    }

    pub fn redo(&mut self) {
        if let Some((next, source)) = self.redo.pop() {
            let cur = std::mem::replace(&mut self.flame, next);
            self.undo.push((cur, std::mem::replace(&mut self.source, source)));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_planes_pick_their_affine() {
        let mut xf = XForm::default();
        edit_affine_mut(&mut xf, EditPlane::YZ, true).c20 = 3.0;
        edit_affine_mut(&mut xf, EditPlane::ZX, false).c21 = 4.0;
        assert_eq!((xf.yz_post.c20, xf.zx_coefs.c21, xf.coefs.c20), (3.0, 4.0, 0.0));
        assert_eq!(edit_affine(&xf, EditPlane::YZ, true).c20, 3.0);
    }

    fn named(name: &str) -> Flame {
        Flame { name: name.into(), ..Flame::default() }
    }

    fn names(v: &[Flame]) -> Vec<&str> {
        v.iter().map(|f| f.name.as_str()).collect()
    }

    #[test]
    fn saving_into_a_multi_flame_file_keeps_the_other_flames() {
        let file = PathBuf::from("/tmp/x.flame");
        let three = || vec![named("a"), named("b"), named("c")];
        let mut doc = Document::new(named("start"));
        doc.load(named("a"), Some(file.clone()));
        // Picked flame 2 of the file in Explore › File and changed it.
        doc.replace(named("b"));
        doc.source = Some((file.clone(), 1));
        doc.flame.name = "b2".into();
        let (flames, i) = doc.flames_for_save(&file, three());
        assert_eq!((names(&flames), i), (vec!["a", "b2", "c"], 1));
        // A flame that is not from the file is appended; saving it again replaces it.
        doc.replace(named("new"));
        let (flames, i) = doc.flames_for_save(&file, three());
        assert_eq!((names(&flames), i), (vec!["a", "b", "c", "new"], 3));
        let (flames, _) = doc.flames_for_save(&file, flames);
        assert_eq!(names(&flames), ["a", "b", "c", "new"]);
        // Undo brings back the (edited) flame from the file together with where it came from.
        doc.undo();
        assert_eq!(doc.flame.name, "b2");
        assert_eq!(doc.source, Some((file.clone(), 1)));
        // Redo returns to the appended flame, which was saved as flame 4.
        doc.redo();
        assert_eq!(doc.source, Some((file.clone(), 3)));
        // A file with one flame (or a new file) gets the editor's flame alone.
        let (flames, i) = doc.flames_for_save(&file, vec![named("only")]);
        assert_eq!((names(&flames), i), (vec!["new"], 0));
        assert_eq!(doc.flames_for_save(std::path::Path::new("/tmp/new.flame"), Vec::new()).0.len(), 1);
    }
}

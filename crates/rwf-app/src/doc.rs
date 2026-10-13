//! The edited document: flame, selection and undo history.

use std::path::PathBuf;

use rwf_core::{Flame, Layer, XForm};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    None,
    XForm(usize),
    Final(usize),
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
        let sel = self.selection;
        let l = self.layer_mut();
        match sel {
            Selection::XForm(i) => l.xforms.get_mut(i),
            Selection::Final(i) => l.final_xforms.get_mut(i),
            Selection::None => None,
        }
    }

    /// Makes sure the selection points to an existing transform.
    pub fn fix_selection(&mut self) {
        self.layer = self.layer.min(self.flame.layers.len().saturating_sub(1));
        let l = self.layer();
        let (nx, nf) = (l.xforms.len(), l.final_xforms.len());
        self.selection = match self.selection {
            Selection::XForm(i) if i < nx => Selection::XForm(i),
            Selection::Final(i) if i < nf => Selection::Final(i),
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

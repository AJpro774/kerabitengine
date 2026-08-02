//! Scene undo/redo via snapshots (levels are small).

use kerabit::Scene;

const MAX_DEPTH: usize = 64;

/// Undo / redo stacks for [`Scene`] mutations.
#[derive(Default)]
pub struct UndoStack {
    undo: Vec<Scene>,
    redo: Vec<Scene>,
    /// When true, the next mutation should record a checkpoint.
    /// Cleared after a continuous edit begins; set again when the gesture ends.
    pub need_checkpoint: bool,
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            need_checkpoint: true,
        }
    }

    /// Record `scene` before a discrete mutation (add / delete / align / …).
    pub fn push(&mut self, scene: &Scene) {
        self.undo.push(scene.clone());
        if self.undo.len() > MAX_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.need_checkpoint = true;
    }

    /// For continuous edits (inspector drag / gizmo): push at most once per gesture.
    pub fn push_if_needed(&mut self, scene: &Scene) {
        if self.need_checkpoint {
            self.push(scene);
            self.need_checkpoint = false;
        }
    }

    /// Call when a continuous edit gesture ends (mouse up, selection change).
    pub fn end_gesture(&mut self) {
        self.need_checkpoint = true;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Restore previous scene; returns the restored snapshot if any.
    pub fn undo(&mut self, current: &Scene) -> Option<Scene> {
        let prev = self.undo.pop()?;
        self.redo.push(current.clone());
        self.need_checkpoint = true;
        Some(prev)
    }

    /// Re-apply a redone scene; returns the restored snapshot if any.
    pub fn redo(&mut self, current: &Scene) -> Option<Scene> {
        let next = self.redo.pop()?;
        self.undo.push(current.clone());
        if self.undo.len() > MAX_DEPTH {
            self.undo.remove(0);
        }
        self.need_checkpoint = true;
        Some(next)
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.need_checkpoint = true;
    }
}

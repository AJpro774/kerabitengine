//! Multi-entity selection (primary = last index).

/// Ordered selection; the last entry is the primary (inspector / gizmo target).
#[derive(Clone, Debug, Default)]
pub struct Selection {
    indices: Vec<usize>,
}

impl Selection {
    pub fn primary(&self) -> Option<usize> {
        self.indices.last().copied()
    }

    pub fn len(&self) -> usize {
        self.indices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn as_slice(&self) -> &[usize] {
        &self.indices
    }

    pub fn contains(&self, index: usize) -> bool {
        self.indices.contains(&index)
    }

    pub fn clear(&mut self) {
        self.indices.clear();
    }

    /// Replace selection with a single entity.
    pub fn set_one(&mut self, index: usize) {
        self.indices.clear();
        self.indices.push(index);
    }

    /// Set selection to these indices (last becomes primary).
    pub fn set_many(&mut self, indices: Vec<usize>) {
        self.indices = indices;
    }

    /// Toggle `index` in the selection (Shift/Cmd click).
    pub fn toggle(&mut self, index: usize) {
        if let Some(pos) = self.indices.iter().position(|&i| i == index) {
            self.indices.remove(pos);
        } else {
            self.indices.push(index);
        }
    }

    /// Drop indices that are out of range after deletions.
    pub fn retain_valid(&mut self, entity_count: usize) {
        self.indices.retain(|&i| i < entity_count);
    }

    /// Remap selection after entity list changes using names.
    pub fn restore_by_names(&mut self, names: &[String], entities: &[kerabit::SceneEntity]) {
        self.indices.clear();
        for name in names {
            if let Some(i) = entities.iter().position(|e| e.name == *name) {
                self.indices.push(i);
            }
        }
    }

    pub fn names(&self, entities: &[kerabit::SceneEntity]) -> Vec<String> {
        self.indices
            .iter()
            .filter_map(|&i| entities.get(i).map(|e| e.name.clone()))
            .collect()
    }

    /// Sorted descending — safe for multi-delete.
    pub fn sorted_desc(&self) -> Vec<usize> {
        let mut v = self.indices.clone();
        v.sort_unstable();
        v.dedup();
        v.reverse();
        v
    }
}

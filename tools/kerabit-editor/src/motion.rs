//! One-shot UI motion. Values decay to rest so the editor does not repaint forever.

/// Selection and status flashes. No idle loops.
#[derive(Clone, Debug, Default)]
pub struct Motion {
    selection: f32,
    status: f32,
}

impl Motion {
    /// Restart the selection flash (hierarchy row, viewport frame).
    pub fn nudge_selection(&mut self) {
        self.selection = 1.0;
    }

    /// Restart the status-bar flash.
    pub fn nudge_status(&mut self) {
        self.status = 1.0;
    }

    /// Advance toward rest. `dt` is seconds.
    pub fn tick(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.05);
        self.selection = (self.selection - dt / 0.28).max(0.0);
        self.status = (self.status - dt / 0.36).max(0.0);
    }

    /// 1 just after a selection change, 0 at rest.
    pub fn selection_glow(&self) -> f32 {
        ease_out(self.selection)
    }

    /// 1 just after a status change that earned feedback, 0 at rest.
    pub fn status_glow(&self) -> f32 {
        ease_out(self.status)
    }

    /// True while a flash still needs frames.
    pub fn busy(&self) -> bool {
        self.selection > 0.001 || self.status > 0.001
    }
}

fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flashes_decay_to_rest() {
        let mut motion = Motion::default();
        assert!(!motion.busy());
        motion.nudge_selection();
        motion.nudge_status();
        assert_eq!(motion.selection_glow(), 1.0);
        assert!(motion.busy());
        for _ in 0..40 {
            motion.tick(1.0 / 60.0);
        }
        assert!(!motion.busy());
        assert_eq!(motion.selection_glow(), 0.0);
        assert_eq!(motion.status_glow(), 0.0);
    }
}

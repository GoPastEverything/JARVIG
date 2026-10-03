//! Editor undo stack. One transaction is one user intent.
//!
//! A transaction stores the save record of each touched entity before and after
//! the edit, plus the world-settings numbers when the environment entity is in
//! the set. Feature history stays inside the block record. This stack does not
//! replay it, and it does not record selection.
//!
//! An open gesture (a drag, a modeling preview, a sculpt stroke) is not an entry.
//! Cancel throws that gesture away. Commit pushes one entry. The stack keeps 64.

use jarvig_core::{AuthoringError, EntityId, EntityMemento, MementoEffect, SceneWorld, WorldSettingsRecord};

pub const HISTORY_LIMIT: usize = 64;

#[derive(Clone, Debug)]
struct Stamp {
    label: String,
    before: Vec<EntityMemento>,
    after: Vec<EntityMemento>,
    settings_before: Option<WorldSettingsRecord>,
    settings_after: Option<WorldSettingsRecord>,
}

#[derive(Clone, Debug)]
struct OpenEdit {
    label: String,
    ids: Vec<EntityId>,
    before: Vec<EntityMemento>,
    settings_before: Option<WorldSettingsRecord>,
}

/// Entity apply plus the world-settings record the editor writes afterwards.
/// Settings are not inside the entity record, so the level capture cannot carry them.
#[derive(Clone, Debug)]
pub struct HistoryApply {
    pub effect: MementoEffect,
    pub settings: Option<WorldSettingsRecord>,
}

#[derive(Clone, Debug, Default)]
pub struct EditHistory {
    undo: Vec<Stamp>,
    redo: Vec<Stamp>,
    open: Option<OpenEdit>,
}

impl EditHistory {
    pub fn gesture_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn open_label(&self) -> Option<&str> {
        self.open.as_ref().map(|edit| edit.label.as_str())
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|stamp| stamp.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|stamp| stamp.label.as_str())
    }

    pub fn can_undo(&self) -> bool {
        self.open.is_some() || !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        self.open.is_none() && !self.redo.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Drops both stacks and any open gesture. Does not write the world.
    /// A loaded level must not receive the previous level's records.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.open = None;
    }

    /// Starts one gesture. A second begin while a gesture is open does nothing,
    /// so a preview cannot nest a move inside itself by accident.
    pub fn begin(&mut self, label: &str, world: &SceneWorld, ids: &[EntityId]) {
        if self.open.is_some() {
            return;
        }
        let mut before = Vec::with_capacity(ids.len());
        let mut settings_before = None;
        for id in ids {
            if settings_before.is_none() {
                settings_before = world.authored_world_settings(*id);
            }
            before.push(memento_of(world, *id));
        }
        self.open = Some(OpenEdit {
            label: label.to_string(),
            ids: ids.to_vec(),
            before,
            settings_before,
        });
    }

    /// The id was created inside the open gesture. Undo must destroy it.
    pub fn note_created(&mut self, id: EntityId) {
        let Some(edit) = &mut self.open else { return };
        if let Some(index) = edit.ids.iter().position(|stored| *stored == id) {
            edit.before[index] = EntityMemento::Absent(id);
        } else {
            edit.ids.push(id);
            edit.before.push(EntityMemento::Absent(id));
        }
    }

    /// Closes the gesture. Pushes one entry when something changed, and drops it when nothing did.
    pub fn commit(&mut self, world: &SceneWorld) -> bool {
        let Some(edit) = self.open.take() else { return false };
        let after: Vec<_> = edit.ids.iter().copied().map(|id| memento_of(world, id)).collect();
        let settings_after = edit.settings_before.as_ref().and_then(|before| world.authored_world_settings(before.entity));
        if after == edit.before && settings_after == edit.settings_before {
            return false;
        }
        self.redo.clear();
        self.undo.push(Stamp {
            label: edit.label,
            before: edit.before,
            after,
            settings_before: edit.settings_before,
            settings_after,
        });
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        true
    }

    /// Puts the before-stamp back and pushes nothing. A failed apply leaves the gesture open.
    pub fn cancel(&mut self, world: &mut SceneWorld) -> Result<Option<HistoryApply>, AuthoringError> {
        let Some(edit) = self.open.take() else { return Ok(None) };
        match world.apply_mementos(&edit.before) {
            Ok(effect) => Ok(Some(HistoryApply { effect, settings: edit.settings_before })),
            Err(error) => {
                self.open = Some(edit);
                Err(error)
            }
        }
    }

    /// Refuses while a gesture is open. The editor cancels that gesture first.
    pub fn undo(&mut self, world: &mut SceneWorld) -> Result<Option<HistoryApply>, AuthoringError> {
        if self.open.is_some() {
            return Ok(None);
        }
        let Some(stamp) = self.undo.pop() else { return Ok(None) };
        match world.apply_mementos(&stamp.before) {
            Ok(effect) => {
                let settings = stamp.settings_before.clone();
                self.redo.push(stamp);
                Ok(Some(HistoryApply { effect, settings }))
            }
            Err(error) => {
                self.undo.push(stamp);
                Err(error)
            }
        }
    }

    pub fn redo(&mut self, world: &mut SceneWorld) -> Result<Option<HistoryApply>, AuthoringError> {
        if self.open.is_some() {
            return Ok(None);
        }
        let Some(stamp) = self.redo.pop() else { return Ok(None) };
        match world.apply_mementos(&stamp.after) {
            Ok(effect) => {
                let settings = stamp.settings_after.clone();
                self.undo.push(stamp);
                Ok(Some(HistoryApply { effect, settings }))
            }
            Err(error) => {
                self.redo.push(stamp);
                Err(error)
            }
        }
    }
}

fn memento_of(world: &SceneWorld, id: EntityId) -> EntityMemento {
    match world.remember_entity(id) {
        Ok(record) => EntityMemento::Present(record),
        Err(_) => EntityMemento::Absent(id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvig_core::{BlockRecord, Vec3};

    #[test]
    fn a_bevel_preview_is_one_undo_and_cancel_pushes_nothing() {
        let mut world = SceneWorld::bootstrap();
        let id = world
            .create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap())
            .unwrap();
        let mut history = EditHistory::default();
        history.begin("Bevel", &world, &[id]);
        world.preview_block_bevel(id, 0.2).unwrap();
        assert!(history.undo(&mut world).unwrap().is_none());
        assert!(history.gesture_open());
        history.cancel(&mut world).unwrap();
        assert_eq!(history.undo_len(), 0);
        let record = world.authored_block(id).unwrap();
        assert!(record.bevel_m.abs() < 1.0e-9);
        assert!(record.history.is_empty());

        history.begin("Bevel", &world, &[id]);
        assert!(!history.commit(&world));
        assert_eq!(history.undo_len(), 0);

        history.begin("Bevel", &world, &[id]);
        world.preview_block_bevel(id, 0.2).unwrap();
        world.commit_block_bevel(id, 0.0).unwrap();
        assert!(history.commit(&world));
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.undo_label(), Some("Bevel"));

        history.undo(&mut world).unwrap();
        let record = world.authored_block(id).unwrap();
        assert!(record.bevel_m.abs() < 1.0e-9);
        assert!(record.history.is_empty());
        assert_eq!(history.undo_len(), 0);

        history.redo(&mut world).unwrap();
        let record = world.authored_block(id).unwrap();
        assert!((record.bevel_m - 0.2).abs() < 1.0e-6);
        assert_eq!(record.history.len(), 1);
        assert_eq!(history.undo_label(), Some("Bevel"));
    }

    #[test]
    fn the_stack_keeps_the_newest_64() {
        let mut world = SceneWorld::bootstrap();
        let id = world
            .create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap())
            .unwrap();
        let mut history = EditHistory::default();
        for index in 0..70 {
            history.begin("Rename", &world, &[id]);
            world.set_entity_name(id, &format!("Block {index}")).unwrap();
            assert!(history.commit(&world));
        }
        assert_eq!(history.undo_len(), HISTORY_LIMIT);
        history.undo(&mut world).unwrap();
        assert_eq!(world.entity_outline().iter().find(|row| row.uuid == id).unwrap().name, "Block 68");
    }
}

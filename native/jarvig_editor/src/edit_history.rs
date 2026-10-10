//! Editor undo stack. One transaction is one user intent.
//!
//! A transaction stores the save record of each touched entity before and after
//! the edit, plus the world-settings numbers when the environment entity is in
//! the set. Feature history stays inside the block record. This stack does not
//! replay it, and it does not record selection.
//!
//! An open gesture (a drag, a modeling preview, a sculpt stroke) is not an entry.
//! Cancel throws that gesture away. Commit pushes one entry. The stack keeps 64.

use jarvig_core::{AuthoringError, EntityId, EntityMemento, MementoEffect, SceneOrganization, SceneWorld, WorldSettingsRecord};

pub const HISTORY_LIMIT: usize = 64;

#[derive(Clone, Debug)]
struct Stamp {
    label: String,
    before: Vec<EntityMemento>,
    after: Vec<EntityMemento>,
    settings_before: Option<WorldSettingsRecord>,
    settings_after: Option<WorldSettingsRecord>,
    organization_before: SceneOrganization,
    organization_after: SceneOrganization,
}

#[derive(Clone, Debug)]
struct OpenEdit {
    label: String,
    ids: Vec<EntityId>,
    before: Vec<EntityMemento>,
    settings_before: Option<WorldSettingsRecord>,
    organization_before: SceneOrganization,
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
            organization_before: world.organization().clone(),
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
        let organization_after = world.organization().clone();
        if after == edit.before && settings_after == edit.settings_before && organization_after == edit.organization_before {
            return false;
        }
        self.redo.clear();
        self.undo.push(Stamp {
            label: edit.label,
            before: edit.before,
            after,
            settings_before: edit.settings_before,
            settings_after,
            organization_before: edit.organization_before,
            organization_after,
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
            Ok(effect) => {
                world.restore_scene_organization(edit.organization_before);
                Ok(Some(HistoryApply { effect, settings: edit.settings_before }))
            }
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
                world.restore_scene_organization(stamp.organization_before.clone());
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
                world.restore_scene_organization(stamp.organization_after.clone());
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
    use jarvig_core::{
        empty_world_level, evaluate_intent_shadow, parse_level, AuthoringResult, BlockOp, BlockRecord, IntentShadowStatus, LevelDocument, SolidBody, TopologyEdit,
        Vec3, BLOCK_EXTRUDE_STEP_M,
    };

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
    fn one_multi_edge_bevel_undoes_and_redoes_as_one_step() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let source = SolidBody::from_box([2.0, 2.0, 2.0]).unwrap();
        let pose = world.entity_local_pose(id).unwrap();
        world.preview_block_body(id, source.clone(), pose.translation).unwrap();
        let mut history = EditHistory::default();
        history.begin("Bevel", &world, &[id]);
        let cut = source.bevel_edges(&[16, 12], 0.2).unwrap();
        let shifted = Vec3::new(
            pose.translation.x + cut.edit.shift[0],
            pose.translation.y + cut.edit.shift[1],
            pose.translation.z + cut.edit.shift[2],
        );
        world.preview_block_body(id, cut.edit.body.clone(), shifted).unwrap();
        world.commit_block_topology(id, BlockOp::BevelEdges { edges: vec![16, 12], distance_m: cut.width_m }).unwrap();
        assert!(history.commit(&world));
        assert_eq!(history.undo_len(), 1);
        let beveled = world.authored_block(id).unwrap().body.clone();
        history.undo(&mut world).unwrap();
        assert_eq!(world.authored_block(id).unwrap().body.as_ref(), Some(&source));
        assert!(world.authored_block(id).unwrap().history.iter().all(|op| !matches!(op, BlockOp::BevelEdges { .. })));
        assert_eq!(history.undo_len(), 0);
        history.redo(&mut world).unwrap();
        assert_eq!(world.authored_block(id).unwrap().body, beveled);
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.undo_label(), Some("Bevel"));
    }

    #[test]
    fn one_loop_bevel_undoes_and_redoes_as_one_step() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        for face in 1..=6 {
            let _ = world.subdivide_block_face(id, face, 2, 2);
        }
        let source = world.authored_block(id).unwrap().body.clone().unwrap();
        let edges = source.edges.iter().map(|edge| edge.id).find_map(|edge| {
            let walk = source.edge_loop(&[edge]);
            if walk.ids.len() < 2 {
                return None;
            }
            source.bevel_edges(&walk.ids, 0.05).ok().map(|_| walk.ids)
        });
        let edges = edges.expect("subdividing the cube opens a bevelable loop");
        let pose = world.entity_local_pose(id).unwrap();
        let cut = source.bevel_edges(&edges, 0.05).unwrap();
        let mut history = EditHistory::default();
        history.begin("Bevel", &world, &[id]);
        let shifted = Vec3::new(pose.translation.x + cut.edit.shift[0], pose.translation.y + cut.edit.shift[1], pose.translation.z + cut.edit.shift[2]);
        world.preview_block_body(id, cut.edit.body.clone(), shifted).unwrap();
        world.commit_block_topology(id, BlockOp::BevelEdges { edges: cut.edges.clone(), distance_m: cut.width_m }).unwrap();
        assert!(history.commit(&world));
        assert_eq!(history.undo_len(), 1);
        assert_eq!(world.authored_block(id).unwrap().history.iter().filter(|op| matches!(op, BlockOp::BevelEdges { .. })).count(), 1);
        let beveled = world.authored_block(id).unwrap().body.clone();
        history.undo(&mut world).unwrap();
        assert_eq!(world.authored_block(id).unwrap().body.as_ref(), Some(&source));
        assert_eq!(history.undo_len(), 0);
        history.redo(&mut world).unwrap();
        assert_eq!(world.authored_block(id).unwrap().body, beveled);
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.undo_label(), Some("Bevel"));
        let after = world.authored_block(id).unwrap().body.unwrap();
        let fresh = after.edges.iter().map(|edge| edge.id).find(|edge| source.edges.iter().all(|old| old.id != *edge)).unwrap();
        history.begin("Split", &world, &[id]);
        assert_eq!(world.split_block_edge(id, fresh).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        assert_eq!(history.undo_len(), 2);
        world.authored_block(id).unwrap().body.unwrap().validate().unwrap();
    }

    #[test]
    fn material_assignment_and_factors_undo_as_two_steps() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let intent = world.authored_block(id).unwrap().intent.len();
        assert_eq!(world.add_block_material_slot(id).unwrap(), AuthoringResult::Applied);
        let mut history = EditHistory::default();
        history.begin("Assign Material", &world, &[id]);
        assert_eq!(world.assign_block_faces(id, &[1, 3], 1).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        let painted = world.authored_block(id).unwrap();
        assert_eq!(painted.intent.len(), intent);
        assert_eq!(painted.faces_on_slot(1), vec![1, 3]);
        history.begin("Material", &world, &[id]);
        assert_eq!(world.set_block_material_factors(id, 1, [0.2, 0.3, 0.4], 0.2, 0.7).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.undo_label(), Some("Material"));
        let colored = world.authored_block(id).unwrap();
        assert_eq!(colored.material_slot(1).unwrap().base_color, [0.2, 0.3, 0.4, 1.0]);
        assert_eq!(colored.face_materials, painted.face_materials);

        history.undo(&mut world).unwrap();
        let restored_color = world.authored_block(id).unwrap();
        assert_eq!(restored_color.face_materials, painted.face_materials);
        assert_eq!(restored_color.material_slot(1).unwrap().base_color, painted.material_slot(1).unwrap().base_color);
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.undo_label(), Some("Assign Material"));

        history.undo(&mut world).unwrap();
        let cleared = world.authored_block(id).unwrap();
        assert!(cleared.face_materials.is_empty());
        assert_eq!(cleared.slot_count(), 2);
        assert_eq!(history.undo_len(), 0);

        history.redo(&mut world).unwrap();
        assert_eq!(world.authored_block(id).unwrap().face_materials, painted.face_materials);
        history.redo(&mut world).unwrap();
        let redone = world.authored_block(id).unwrap();
        assert_eq!(redone.face_materials, painted.face_materials);
        assert_eq!(redone.material_slot(1).unwrap().base_color, [0.2, 0.3, 0.4, 1.0]);
        assert!((redone.material_slot(1).unwrap().roughness - 0.2).abs() < 1.0e-6);
        assert!((redone.material_slot(1).unwrap().metallic - 0.7).abs() < 1.0e-6);
        assert_eq!(redone.body, colored.body);
        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.undo_label(), Some("Material"));
    }

    #[test]
    fn make_unique_undoes_and_a_shared_color_is_one_step() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let intent = world.authored_block(id).unwrap().intent.len();
        let gray = world.authored_block(id).unwrap().material.clone();
        let mut history = EditHistory::default();
        history.begin("Face Material", &world, &[id]);
        assert_eq!(world.make_block_faces_unique(id, &[1, 3]).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        let unique = world.authored_block(id).unwrap();
        assert_eq!(unique.intent.len(), intent);
        assert!(unique.body.is_none());
        assert_eq!(unique.slot_count(), 2);
        assert_eq!(unique.faces_on_slot(1), vec![1, 3]);
        assert_eq!(unique.faces_on_slot(0), vec![2, 4, 5, 6]);
        assert_eq!(unique.material_slot(1).unwrap().base_color, gray.base_color);
        assert_eq!(unique.material_slot(1).unwrap().roughness, gray.roughness);
        assert_eq!(unique.material_slot(1).unwrap().metallic, gray.metallic);
        assert!(unique.face_materials.iter().any(|entry| entry.face == Some(1) && entry.provenance == vec!["F:seed/0".to_string()]));
        assert!(unique.face_materials.iter().any(|entry| entry.face == Some(3) && entry.provenance == vec!["F:seed/2".to_string()]));
        history.undo(&mut world).unwrap();
        let cleared = world.authored_block(id).unwrap();
        assert_eq!(cleared.slot_count(), 1);
        assert!(cleared.face_materials.is_empty());
        assert_eq!(cleared.material.base_color, gray.base_color);
        assert_eq!(history.undo_len(), 0);

        history.begin("Face Material", &world, &[id]);
        assert_eq!(world.make_block_faces_unique(id, &[2]).unwrap(), AuthoringResult::Applied);
        let slot = world.authored_block(id).unwrap().slot_count() - 1;
        assert_eq!(world.set_block_material_factors(id, slot, [0.0, 0.2, 0.9], 0.25, 0.8).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.undo_label(), Some("Face Material"));
        let painted = world.authored_block(id).unwrap();
        assert_eq!(painted.intent.len(), intent);
        assert_eq!(painted.bound_slot(2), slot);
        assert_eq!(painted.bound_slot(1), 0);
        assert_eq!(painted.material.base_color, gray.base_color);
        assert_eq!(painted.material_slot(slot).unwrap().base_color, [0.0, 0.2, 0.9, 1.0]);
        assert!((painted.material_slot(slot).unwrap().roughness - 0.25).abs() < 1.0e-6);
        assert!((painted.material_slot(slot).unwrap().metallic - 0.8).abs() < 1.0e-6);
        history.undo(&mut world).unwrap();
        let restored = world.authored_block(id).unwrap();
        assert_eq!(restored.slot_count(), 1);
        assert!(restored.face_materials.is_empty());
        assert_eq!(restored.material.base_color, gray.base_color);
        history.redo(&mut world).unwrap();
        let redone = world.authored_block(id).unwrap();
        assert_eq!(redone.bound_slot(2), 1);
        assert_eq!(redone.bound_slot(1), 0);
        assert_eq!(redone.material.base_color, gray.base_color);
        assert_eq!(redone.material_slot(1).unwrap().base_color, [0.0, 0.2, 0.9, 1.0]);
        assert_eq!(redone.intent.len(), intent);
    }

    #[test]
    fn upper_rim_undoes_as_one_surface_group() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let intent = world.authored_block(id).unwrap().intent.len();
        let mut history = EditHistory::default();
        history.begin("Surface Group", &world, &[id]);
        assert_eq!(world.create_block_surface_group(id, "Upper Rim", &[3, 5]).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.undo_label(), Some("Surface Group"));
        let grouped = world.authored_block(id).unwrap();
        assert_eq!(grouped.intent.len(), intent);
        assert!(grouped.body.is_none());
        assert_eq!(grouped.surface_groups.len(), 1);
        assert_eq!(grouped.surface_groups[0].name, "Upper Rim");
        assert_eq!(grouped.slot_count(), 2);
        assert_eq!(grouped.resolved_group_faces(grouped.surface_groups[0].id), vec![3, 5]);
        history.undo(&mut world).unwrap();
        let cleared = world.authored_block(id).unwrap();
        assert!(cleared.surface_groups.is_empty());
        assert_eq!(cleared.slot_count(), 1);
        assert!(cleared.face_materials.is_empty());
        assert!(cleared.body.is_none());
        assert_eq!(cleared.intent.len(), intent);
        assert_eq!(history.undo_len(), 0);
        history.redo(&mut world).unwrap();
        let redone = world.authored_block(id).unwrap();
        assert_eq!(redone.surface_groups[0].name, "Upper Rim");
        assert_eq!(redone.resolved_group_faces(redone.surface_groups[0].id), vec![3, 5]);
        assert_eq!(redone.slot_count(), 2);
        assert!(redone.body.is_none());
        assert_eq!(redone.intent.len(), intent);
    }

    #[test]
    fn folder_edits_undo_without_moving_the_solid_or_its_group() {
        let mut world = SceneWorld::bootstrap();
        let crate_id = world.create_block(Vec3::new(1.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let lamp = world.create_block(Vec3::new(3.0, 1.0, -4.0), BlockRecord::standard([1.0, 1.0, 1.0]).unwrap()).unwrap();
        world.set_entity_name(crate_id, "Crate").unwrap();
        world.create_block_surface_group(crate_id, "Upper Rim", &[3, 5]).unwrap();
        let pose = world.entity_local_pose(crate_id).unwrap().translation;
        let parent = world.entity_parent(crate_id).unwrap();
        let before = world.authored_block(crate_id).unwrap();
        let mut history = EditHistory::default();

        history.begin("Create Folder", &world, &[]);
        let environment = world.create_scene_folder("Environment", None).unwrap();
        let architecture = world.create_scene_folder("Architecture", Some(environment)).unwrap();
        assert!(history.commit(&world));
        history.begin("Rename Folder", &world, &[]);
        assert_eq!(world.rename_scene_folder(architecture, "Structures").unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        history.begin("Move To Folder", &world, &[]);
        world.move_entity_to_folder(crate_id, Some(architecture)).unwrap();
        world.move_entity_to_folder(lamp, Some(environment)).unwrap();
        assert!(history.commit(&world));
        assert_eq!(history.undo_label(), Some("Move To Folder"));

        history.undo(&mut world).unwrap();
        assert_eq!(world.scene_folder_of(crate_id), None);
        assert_eq!(world.scene_folder_of(lamp), None);
        assert_eq!(world.organization().folder(architecture).unwrap().name, "Structures");
        history.undo(&mut world).unwrap();
        assert_eq!(world.organization().folder(architecture).unwrap().name, "Architecture");
        history.undo(&mut world).unwrap();
        assert!(world.organization().folders().is_empty());
        history.redo(&mut world).unwrap();
        history.redo(&mut world).unwrap();
        history.redo(&mut world).unwrap();
        assert_eq!(world.scene_folder_of(crate_id), Some(architecture));
        assert_eq!(world.scene_folder_of(lamp), Some(environment));
        assert_eq!(world.organization().folder(architecture).unwrap().name, "Structures");

        history.begin("Delete Folder", &world, &[]);
        world.delete_scene_folder(architecture).unwrap();
        assert!(history.commit(&world));
        assert_eq!(world.scene_folder_of(crate_id), Some(environment));
        history.undo(&mut world).unwrap();
        assert_eq!(world.scene_folder_of(crate_id), Some(architecture));
        assert!(world.authored_block(crate_id).is_some());

        let restored = world.authored_block(crate_id).unwrap();
        assert_eq!(world.entity_local_pose(crate_id).unwrap().translation, pose);
        assert_eq!(world.entity_parent(crate_id).unwrap(), parent);
        assert_eq!(restored.body, before.body);
        assert_eq!(restored.materials, before.materials);
        assert_eq!(restored.face_materials, before.face_materials);
        assert_eq!(restored.surface_groups, before.surface_groups);
        assert_eq!(restored.intent.len(), before.intent.len());
        assert_eq!(restored.surface_groups[0].name, "Upper Rim");
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

    #[test]
    fn undo_and_redo_keep_the_shadow_equivalent() {
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let mut history = EditHistory::default();
        history.begin("Create Block", &world, &[]);
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        history.note_created(id);
        assert!(history.commit(&world));

        history.begin("Subdivide", &world, &[id]);
        assert_eq!(world.subdivide_block_face(id, 5, 2, 2).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        let divided = stored(&world, id);
        let cell = divided
            .faces
            .iter()
            .find(|face| face.id != 5 && divided.unit_normal(face.id).is_some_and(|normal| normal[2] > 0.9))
            .map(|face| face.id)
            .unwrap();
        let normal = divided.unit_normal(cell).unwrap();
        history.begin("Extrude", &world, &[id]);
        assert_eq!(
            world
                .extrude_block_faces(id, &[cell], [normal[0] * BLOCK_EXTRUDE_STEP_M, normal[1] * BLOCK_EXTRUDE_STEP_M, normal[2] * BLOCK_EXTRUDE_STEP_M])
                .unwrap(),
            AuthoringResult::Applied
        );
        assert!(history.commit(&world));

        let edge = stored(&world, id).edges.iter().map(|edge| edge.id).find(|edge| *edge > 20).unwrap();
        let before: Vec<u32> = stored(&world, id).vertices.iter().map(|vertex| vertex.id).collect();
        history.begin("Split", &world, &[id]);
        assert_eq!(world.split_block_edge(id, edge).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        let vertex = stored(&world, id).vertices.iter().map(|vertex| vertex.id).find(|vertex| !before.contains(vertex)).unwrap();
        let delta = [0.02, 0.0, 0.0];
        let edit = stored(&world, id).move_vertex(vertex, delta).expect("created vertex moves");
        history.begin("Move Vertex", &world, &[id]);
        commit_body(&mut world, id, edit, BlockOp::MoveVertex { vertex, delta_m: delta });
        assert!(history.commit(&world));

        let current = stored(&world, id);
        let cut = current.edges.iter().find_map(|edge| current.bevel_edges(&[edge.id], 0.05).ok()).expect("bevel");
        history.begin("Bevel", &world, &[id]);
        commit_body(&mut world, id, cut.edit, BlockOp::BevelEdges { edges: cut.edges, distance_m: cut.width_m });
        assert!(history.commit(&world));
        history.begin("Size", &world, &[id]);
        assert_eq!(world.set_block_extent(id, 0, 3.0).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        assert_shadow(&world, id);

        let depth = history.undo_len();
        for _ in 0..depth {
            history.undo(&mut world).unwrap();
            if world.authored_block(id).is_some() {
                assert_shadow(&world, id);
            }
        }
        assert!(world.authored_block(id).is_none());
        for _ in 0..depth {
            history.redo(&mut world).unwrap();
            assert_shadow(&world, id);
        }

        history.begin("Create Plane", &world, &[]);
        let plane = world.create_plane(Vec3::new(2.5, 1.0, -4.0)).unwrap();
        history.note_created(plane);
        assert!(history.commit(&world));
        history.begin("Subdivide Plane", &world, &[plane]);
        assert_eq!(world.subdivide_block_face(plane, 3, 2, 2).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        history.begin("Size Plane", &world, &[plane]);
        assert_eq!(world.set_block_extent(plane, 0, 3.0).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        assert_shadow(&world, plane);
        for _ in 0..3 {
            history.undo(&mut world).unwrap();
            if world.authored_block(plane).is_some() {
                assert_shadow(&world, plane);
            }
        }
        assert!(world.authored_block(plane).is_none());
        assert_shadow(&world, id);
        for _ in 0..3 {
            history.redo(&mut world).unwrap();
            assert_shadow(&world, plane);
        }
        assert_shadow(&world, id);

        let mut json = LevelDocument::capture(&world, document.level_uuid, "Undo").unwrap().to_json();
        for _ in 0..2 {
            let loaded = parse_level(&json).unwrap().instantiate().unwrap();
            assert_shadow(&loaded, id);
            assert_shadow(&loaded, plane);
            assert_eq!(loaded.authored_block(id).unwrap().material.name, world.authored_block(id).unwrap().material.name);
            assert_eq!(loaded.authored_block(plane).unwrap().material.name, "standard_white");
            json = LevelDocument::capture(&loaded, document.level_uuid, "Undo").unwrap().to_json();
        }
    }

    #[test]
    fn a_round_apply_undoes_to_the_sharp_edge_and_redoes_the_curve() {
        let bytes = std::fs::read(jarvig_core::authored_main_level()).expect("main level");
        let parsed = parse_level(std::str::from_utf8(&bytes).unwrap()).unwrap();
        let sharp = parsed
            .entities
            .iter()
            .find(|entity| entity.name == "Intent Solid")
            .and_then(|entity| {
                entity.components.iter().find_map(|component| match component {
                    jarvig_core::ComponentRecord::ParametricBlock(block) => Some(block.clone()),
                    _ => None,
                })
            })
            .expect("Intent Solid");
        let token = "E:grid(F:seed/4,1,1,2)";
        let bindings = jarvig_core::semantic_edge_bindings(&sharp).unwrap();
        let edge_id = bindings.iter().find(|(name, _)| name == token).map(|(_, id)| *id).unwrap();
        let body = match jarvig_core::intent_authority_candidate(&sharp) {
            jarvig_core::IntentAuthorityCandidate::Reconstructable(body) => body,
            jarvig_core::IntentAuthorityCandidate::Refused(reason) => panic!("{reason}"),
        };
        let maximum = jarvig_core::maximum_round_radius(&body, edge_id).unwrap();
        let radius = if maximum >= 1.217 { 1.217 } else { maximum };
        let committed = jarvig_core::commit_class_c_intent(&sharp, jarvig_core::round_entry(token, radius)).unwrap();
        assert!(committed.body.is_none());
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        assert_eq!(world.replace_block_record(id, sharp.clone()).unwrap(), AuthoringResult::Applied);
        let mut history = EditHistory::default();
        history.begin("Round", &world, &[id]);
        assert_eq!(world.replace_block_record(id, committed.clone()).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        assert_eq!(history.undo_label(), Some("Round"));
        assert!((jarvig_core::replay_round(&world.authored_block(id).unwrap()).unwrap().radius_m - radius).abs() < 1.0e-12);
        history.undo(&mut world).unwrap();
        assert!(jarvig_core::replay_round(&world.authored_block(id).unwrap()).is_err());
        history.redo(&mut world).unwrap();
        let redone = world.authored_block(id).unwrap();
        assert!(redone.body.is_none());
        assert!((jarvig_core::replay_round(&redone).unwrap().radius_m - radius).abs() < 1.0e-12);
        let mut captured = LevelDocument::capture(&world, document.level_uuid, "Round").unwrap();
        let solid = captured.entities.iter_mut().find(|entity| entity.uuid == id).unwrap();
        solid.name = "Intent Solid".into();
        let json = captured.to_json();
        let loaded = parse_level(&json).unwrap();
        let loaded_block = loaded
            .entities
            .iter()
            .find(|entity| entity.name == "Intent Solid")
            .and_then(|entity| {
                entity.components.iter().find_map(|component| match component {
                    jarvig_core::ComponentRecord::ParametricBlock(block) => Some(block.clone()),
                    _ => None,
                })
            })
            .unwrap();
        assert!((jarvig_core::replay_round(&loaded_block).unwrap().radius_m - radius).abs() < 1.0e-12);
        assert!(loaded_block.body.is_none());
    }

    #[test]
    fn a_round_set_undoes_both_edges_in_one_step() {
        let bytes = std::fs::read(jarvig_core::authored_main_level()).expect("main level");
        let parsed = parse_level(std::str::from_utf8(&bytes).unwrap()).unwrap();
        let sharp = parsed
            .entities
            .iter()
            .find(|entity| entity.name == "Intent Solid")
            .and_then(|entity| {
                entity.components.iter().find_map(|component| match component {
                    jarvig_core::ComponentRecord::ParametricBlock(block) => Some(block.clone()),
                    _ => None,
                })
            })
            .expect("Intent Solid");
        let tokens = vec!["E:seed-edge/2".to_string(), "E:seed-edge/3".to_string()];
        let committed = jarvig_core::commit_class_c_intent(&sharp, jarvig_core::round_entry_set(&tokens, jarvig_core::CURVE_RADIUS_M)).expect("set");
        assert!(committed.body.is_none());
        let document = empty_world_level();
        let mut world = document.instantiate().unwrap();
        let id = world.create_block(Vec3::new(0.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        assert_eq!(world.replace_block_record(id, sharp).unwrap(), AuthoringResult::Applied);
        let mut history = EditHistory::default();
        history.begin("Round", &world, &[id]);
        assert_eq!(world.replace_block_record(id, committed).unwrap(), AuthoringResult::Applied);
        assert!(history.commit(&world));
        let rounds = jarvig_core::replay_rounds(&world.authored_block(id).unwrap()).unwrap();
        assert_eq!(rounds.len(), 2);
        assert!(rounds.iter().any(|round| round.token == "E:seed-edge/2"));
        assert!(rounds.iter().any(|round| round.token == "E:seed-edge/3"));
        history.undo(&mut world).unwrap();
        assert!(jarvig_core::replay_rounds(&world.authored_block(id).unwrap()).is_err());
        history.redo(&mut world).unwrap();
        let redone = jarvig_core::replay_rounds(&world.authored_block(id).unwrap()).unwrap();
        assert_eq!(redone.len(), 2);
        assert!(redone.iter().all(|round| (round.radius_m - jarvig_core::CURVE_RADIUS_M).abs() < 1.0e-12));
        assert!(world.authored_block(id).unwrap().body.is_none());
    }

    fn stored(world: &SceneWorld, id: jarvig_core::EntityId) -> SolidBody {
        world.authored_block(id).unwrap().body.unwrap()
    }

    fn commit_body(world: &mut SceneWorld, id: jarvig_core::EntityId, edit: TopologyEdit, op: BlockOp) {
        let local = world.entity_local_pose(id).unwrap();
        let translation = Vec3::new(local.translation.x + edit.shift[0], local.translation.y + edit.shift[1], local.translation.z + edit.shift[2]);
        assert_eq!(world.preview_block_body(id, edit.body, translation).unwrap(), AuthoringResult::Applied);
        assert_eq!(world.commit_block_topology(id, op).unwrap(), AuthoringResult::Applied);
    }

    fn assert_shadow(world: &SceneWorld, id: jarvig_core::EntityId) {
        let record = world.authored_block(id).unwrap();
        if let Some(body) = &record.body {
            body.validate().unwrap();
        }
        let report = evaluate_intent_shadow(&record);
        assert_eq!(report.status, IntentShadowStatus::Pass, "{}", report.text);
        let center = world.entity_local_pose(id).unwrap().translation;
        let pushed = world.separate_from_blocks(center);
        let gap = [center.x - pushed.x, center.y - pushed.y, center.z - pushed.z];
        assert!(gap[0] * gap[0] + gap[1] * gap[1] + gap[2] * gap[2] > 0.0025);
    }
}

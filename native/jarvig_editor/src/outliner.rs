//! Derived world outliner. Editor state only.
//!
//! The registry owns the entities. This model is a view: names, parent links by
//! uuid, and which rows are expanded. It does not store a window handle, a
//! bootstrap slot, or a render instance. The tree caret is not editor selection.

use std::collections::{BTreeSet, HashMap};

use jarvig_core::{EntityOutlineInfo, EntityUuid};

/// World is an editor row. It is not an entity and it has no uuid.
/// A folder is scene organization. It is not a geometric parent.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum OutlinerNodeId {
    WorldRoot,
    Entity(EntityUuid),
    Folder(u32),
}

/// One organizational folder in the outliner view. The level owns the record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlinerFolderNode {
    pub id: u32,
    pub name: String,
    pub parent: Option<u32>,
    pub child_folders: Vec<u32>,
    pub entities: Vec<EntityUuid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlinerEntityNode {
    pub entity: EntityUuid,
    pub display_name: String,
    /// Mesh, Light, Probe, World, or empty. Not a second identity.
    pub category: String,
    pub parent: Option<EntityUuid>,
    pub children: Vec<EntityUuid>,
}

/// In-memory view of [`EntityOutlineInfo`]. Expansion is not scene data.
#[derive(Clone, Debug, Default)]
pub struct WorldOutlinerModel {
    revision: u64,
    nodes: Vec<OutlinerEntityNode>,
    roots: Vec<EntityUuid>,
    expanded: BTreeSet<EntityUuid>,
    world_expanded: bool,
    folders: Vec<OutlinerFolderNode>,
    folder_roots: Vec<u32>,
    expanded_folders: BTreeSet<u32>,
    known_folders: BTreeSet<u32>,
    caret: Option<OutlinerNodeId>,
}

/// Empty names still need a label. The uuid stays the identity either way.
pub fn presentation_name(name: &str) -> &str {
    if name.is_empty() { "Unnamed" } else { name }
}

/// Tree text. The category is a suffix, not the row identity. An empty class has no suffix.
pub fn presentation_row(name: &str, category: &str) -> String {
    if category.is_empty() { name.to_string() } else { format!("{name}          {category}") }
}

impl WorldOutlinerModel {
    pub fn empty() -> Self {
        Self { world_expanded: true, ..Self::default() }
    }

    /// Rebuild from one hierarchy pass. Sibling order is the order of `rows`.
    /// That order is the registry insertion order, not a hash-map walk and not
    /// alphabetical. New entities start expanded. A missing caret is cleared.
    pub fn derive(revision: u64, rows: &[EntityOutlineInfo], previous: &Self) -> Self {
        let mut index: HashMap<EntityUuid, usize> = HashMap::with_capacity(rows.len());
        let mut nodes = Vec::with_capacity(rows.len());
        for row in rows {
            if index.contains_key(&row.uuid) {
                continue;
            }
            index.insert(row.uuid, nodes.len());
            nodes.push(OutlinerEntityNode {
                entity: row.uuid,
                display_name: presentation_name(&row.name).to_string(),
                category: row.class.label().to_string(),
                parent: None,
                children: Vec::new(),
            });
        }
        let mut roots = Vec::new();
        for row in rows {
            let Some(&slot) = index.get(&row.uuid) else {
                continue;
            };
            match row.parent.and_then(|parent| index.get(&parent).copied()) {
                Some(parent_slot) => {
                    let parent = nodes[parent_slot].entity;
                    nodes[parent_slot].children.push(row.uuid);
                    nodes[slot].parent = Some(parent);
                }
                None => roots.push(row.uuid),
            }
        }
        let live: BTreeSet<EntityUuid> = nodes.iter().map(|node| node.entity).collect();
        let mut expanded = previous.expanded.intersection(&live).copied().collect::<BTreeSet<_>>();
        for entity in &live {
            if !previous.expanded.contains(entity) && previous.nodes.iter().all(|node| node.entity != *entity) {
                expanded.insert(*entity);
            }
        }
        let caret = previous.caret.filter(|caret| match caret {
            OutlinerNodeId::WorldRoot => true,
            OutlinerNodeId::Entity(entity) => live.contains(entity),
            OutlinerNodeId::Folder(_) => true,
        });
        Self {
            revision,
            nodes,
            roots,
            expanded,
            world_expanded: previous.world_expanded || previous.nodes.is_empty(),
            folders: Vec::new(),
            folder_roots: Vec::new(),
            expanded_folders: previous.expanded_folders.clone(),
            known_folders: previous.known_folders.clone(),
            caret,
        }
    }

    /// Places organizational folders around entities that have no spatial parent.
    ///
    /// A spatially parented entity stays under that parent. Folder membership does not move it.
    /// A folder that was not in the previous view starts expanded.
    /// A saved open or closed state is editor workspace data, applied with `apply_folder_expansion`.
    pub fn place_folders(&mut self, source: &jarvig_core::SceneOrganization) {
        let mut folders: Vec<OutlinerFolderNode> = source
            .folders()
            .iter()
            .map(|folder| OutlinerFolderNode {
                id: folder.id,
                name: folder.name.clone(),
                parent: folder.parent,
                child_folders: Vec::new(),
                entities: Vec::new(),
            })
            .collect();
        folders.sort_by_key(|folder| folder.id);
        let order: Vec<u32> = folders.iter().map(|folder| folder.id).collect();
        for id in order {
            let parent = folders.iter().find(|folder| folder.id == id).and_then(|folder| folder.parent);
            if let Some(parent) = parent {
                if let Some(slot) = folders.iter().position(|folder| folder.id == parent) {
                    folders[slot].child_folders.push(id);
                }
            }
        }
        let root_order = self.roots.clone();
        let mut placed = BTreeSet::new();
        for entity in root_order {
            let Some(folder) = source.folder_of(entity) else { continue };
            let Some(slot) = folders.iter().position(|stored| stored.id == folder) else { continue };
            folders[slot].entities.push(entity);
            placed.insert(entity);
        }
        self.roots.retain(|entity| !placed.contains(entity));
        let live: BTreeSet<u32> = folders.iter().map(|folder| folder.id).collect();
        for id in &live {
            if !self.known_folders.contains(id) {
                self.expanded_folders.insert(*id);
            }
        }
        self.expanded_folders.retain(|id| live.contains(id));
        self.known_folders = live;
        self.folder_roots = folders.iter().filter(|folder| folder.parent.is_none()).map(|folder| folder.id).collect();
        self.folders = folders;
        if let Some(OutlinerNodeId::Folder(id)) = self.caret {
            if self.folder(id).is_none() {
                self.caret = None;
            }
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Names, classes, parents, and sibling order. Expansion and the caret are not rows.
    /// A pose edit revises the world and still returns true.
    pub fn same_rows(&self, other: &Self) -> bool {
        self.nodes == other.nodes && self.roots == other.roots && self.folders == other.folders && self.folder_roots == other.folder_roots
    }

    pub fn entity_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn root_entity_count(&self) -> usize {
        self.roots.len()
    }

    pub fn roots(&self) -> &[EntityUuid] {
        &self.roots
    }

    pub fn node(&self, entity: EntityUuid) -> Option<&OutlinerEntityNode> {
        self.nodes.iter().find(|node| node.entity == entity)
    }

    pub fn display_name(&self, entity: EntityUuid) -> Option<&str> {
        self.node(entity).map(|node| node.display_name.as_str())
    }

    /// What the tree control shows. `display_name` stays the entity name.
    pub fn row_text(&self, entity: EntityUuid) -> Option<String> {
        let node = self.node(entity)?;
        Some(presentation_row(&node.display_name, &node.category))
    }

    pub fn children(&self, entity: EntityUuid) -> &[EntityUuid] {
        self.node(entity).map(|node| node.children.as_slice()).unwrap_or(&[])
    }

    pub fn is_expanded(&self, entity: EntityUuid) -> bool {
        self.expanded.contains(&entity)
    }

    pub fn set_expanded(&mut self, entity: EntityUuid, expanded: bool) {
        if self.node(entity).is_none() {
            return;
        }
        if expanded {
            self.expanded.insert(entity);
        } else {
            self.expanded.remove(&entity);
        }
    }

    pub fn world_expanded(&self) -> bool {
        self.world_expanded
    }

    pub fn set_world_expanded(&mut self, expanded: bool) {
        self.world_expanded = expanded;
    }

    /// Replaces the class suffix. The entity id stays the row.
    pub fn set_category(&mut self, entity: EntityUuid, category: &str) {
        if let Some(node) = self.nodes.iter_mut().find(|node| node.entity == entity) {
            node.category = category.to_string();
        }
    }

    pub fn caret(&self) -> Option<OutlinerNodeId> {
        self.caret
    }

    pub fn set_caret(&mut self, caret: Option<OutlinerNodeId>) {
        self.caret = caret.filter(|caret| match caret {
            OutlinerNodeId::WorldRoot => true,
            OutlinerNodeId::Entity(entity) => self.node(*entity).is_some(),
            OutlinerNodeId::Folder(id) => self.folder(*id).is_some(),
        });
    }

    pub fn folder_roots(&self) -> &[u32] {
        &self.folder_roots
    }

    pub fn folder(&self, id: u32) -> Option<&OutlinerFolderNode> {
        self.folders.iter().find(|folder| folder.id == id)
    }

    pub fn is_folder_expanded(&self, id: u32) -> bool {
        self.expanded_folders.contains(&id)
    }

    pub fn set_folder_expanded(&mut self, id: u32, expanded: bool) {
        if self.folder(id).is_none() {
            return;
        }
        if expanded {
            self.expanded_folders.insert(id);
        } else {
            self.expanded_folders.remove(&id);
        }
    }

    /// Drops remembered folder open state so the next placement starts expanded.
    pub fn reset_folder_expansion(&mut self) {
        self.expanded_folders.clear();
        self.known_folders.clear();
    }

    pub fn folder_ids(&self) -> Vec<u32> {
        self.folders.iter().map(|folder| folder.id).collect()
    }

    pub fn expanded_folder_ids(&self) -> Vec<u32> {
        self.folder_ids().into_iter().filter(|id| self.expanded_folders.contains(id)).collect()
    }

    /// `known` folders use `open`. A folder the workspace has not seen starts expanded.
    pub fn apply_folder_expansion(&mut self, known: &[u32], open: &[u32]) {
        let live = self.folder_ids();
        self.expanded_folders.clear();
        for id in live {
            let expanded = if known.contains(&id) { open.contains(&id) } else { true };
            if expanded {
                self.expanded_folders.insert(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvig_core::{RenderFrameId, SceneWorld};

    fn model_of(world: &SceneWorld, previous: &WorldOutlinerModel) -> WorldOutlinerModel {
        WorldOutlinerModel::derive(world.revision(), &world.entity_outline(), previous)
    }

    #[test]
    fn world_root_is_not_an_entity_and_bootstrap_names_follow_insertion_order() {
        let world = SceneWorld::bootstrap();
        let model = model_of(&world, &WorldOutlinerModel::empty());
        assert!(!matches!(OutlinerNodeId::WorldRoot, OutlinerNodeId::Entity(_)));
        assert_eq!(model.entity_count(), 7);
        assert_eq!(model.root_entity_count(), 7);
        assert_eq!(model.display_name(model.roots()[0]), Some("Near Triangle"));
        assert_eq!(model.display_name(model.roots()[1]), Some("Far Triangle"));
        assert_eq!(model.node(model.roots()[0]).unwrap().category, "Mesh");
        assert_eq!(model.node(model.roots()[2]).unwrap().category, "Light");
        assert_eq!(model.node(model.roots()[3]).unwrap().category, "Light");
        assert_eq!(model.node(model.roots()[4]).unwrap().category, "Light");
        assert_eq!(model.node(model.roots()[5]).unwrap().category, "Probe");
        assert_eq!(model.node(model.roots()[6]).unwrap().category, "World");
        assert_eq!(model.row_text(model.roots()[0]).as_deref(), Some("Near Triangle          Mesh"));
        assert_eq!(model.display_name(model.roots()[5]), Some("Reflection Probe"));
        assert_eq!(model.display_name(model.roots()[6]), Some("World Settings"));
        assert_eq!(model.entity_count(), world.entity_count());
        for root in model.roots() {
            let node = model.node(*root).unwrap();
            assert!(world.component_stack(node.entity).is_ok());
            assert_ne!(node.category, "Transform");
            assert_ne!(node.category, "Components");
        }
        assert_ne!(model.roots()[0], model.roots()[1]);
        assert!(model.display_name(model.roots()[0]).unwrap() != "Object 1");
        assert_eq!(presentation_name(""), "Unnamed");
    }

    #[test]
    fn a_pose_edit_keeps_the_same_rows_and_a_rename_does_not() {
        let mut world = SceneWorld::bootstrap();
        let before = model_of(&world, &WorldOutlinerModel::empty());
        let near = world.entity_outline()[0].uuid;
        world.set_entity_local_translation(near, jarvig_core::Vec3::new(1.25, 0.0, -2.0)).unwrap();
        let moved = model_of(&world, &before);
        assert_ne!(before.revision(), moved.revision());
        assert!(before.same_rows(&moved));
        world.set_entity_name(near, "Moved Card").unwrap();
        let renamed = model_of(&world, &moved);
        assert!(!moved.same_rows(&renamed));
        assert_eq!(renamed.display_name(near), Some("Moved Card"));
    }

    #[test]
    fn a_non_renderable_entity_is_a_row_and_not_a_render_instance() {
        let mut world = SceneWorld::bootstrap();
        let marker = world.create_entity("Gameplay Marker");
        let marker_id = world.resolve(marker).unwrap();
        let model = model_of(&world, &WorldOutlinerModel::empty());
        assert_eq!(model.entity_count(), 8);
        assert_eq!(world.object_count(), 2);
        assert_eq!(model.display_name(marker_id), Some("Gameplay Marker"));
        assert_eq!(model.node(marker_id).unwrap().category, "");
        assert_eq!(model.row_text(marker_id).as_deref(), Some("Gameplay Marker"));
        assert!(model.roots().contains(&marker_id));
        let snapshot = world.extract(RenderFrameId(1)).unwrap();
        assert_eq!(snapshot.instance_count(), 2);
        assert!(snapshot.instances().iter().all(|instance| instance.entity != marker_id));
    }

    #[test]
    fn same_names_stay_distinct_rename_keeps_the_uuid_and_duplicate_mints_one() {
        let mut world = SceneWorld::bootstrap();
        let first = world.create_entity("Light");
        let first_id = world.resolve(first).unwrap();
        let mut model = model_of(&world, &WorldOutlinerModel::empty());
        model.set_caret(Some(OutlinerNodeId::Entity(first_id)));
        model.set_expanded(first_id, false);
        world.rename_entity(first, "Key Light").unwrap();
        assert_eq!(world.resolve(first).unwrap(), first_id);
        model = model_of(&world, &model);
        assert_eq!(model.display_name(first_id), Some("Key Light"));
        assert_eq!(model.caret(), Some(OutlinerNodeId::Entity(first_id)));
        assert!(!model.is_expanded(first_id));
        let second = world.duplicate_entity(first).unwrap();
        let second_id = world.resolve(second).unwrap();
        assert_ne!(second_id, first_id);
        world.rename_entity(second, "Light").unwrap();
        world.rename_entity(first, "Light").unwrap();
        model = model_of(&world, &model);
        assert_eq!(model.display_name(first_id), Some("Light"));
        assert_eq!(model.display_name(second_id), Some("Light"));
        assert_ne!(first_id, second_id);
        assert_eq!(model.entity_count(), 9);
    }

    #[test]
    fn parent_hierarchy_cycle_rejection_and_stale_reuse_do_not_resurrect_a_uuid() {
        let mut world = SceneWorld::bootstrap();
        let ship = world.create_entity("Ship");
        let hull = world.create_entity("Hull");
        let ship_id = world.resolve(ship).unwrap();
        let hull_id = world.resolve(hull).unwrap();
        world.reparent_entity(hull, Some(ship)).unwrap();
        let mut model = model_of(&world, &WorldOutlinerModel::empty());
        assert_eq!(model.node(hull_id).unwrap().parent, Some(ship_id));
        assert_eq!(model.children(ship_id), &[hull_id]);
        assert!(!model.roots().contains(&hull_id));
        assert!(world.reparent_entity(ship, Some(hull)).is_err());
        model = model_of(&world, &model);
        assert_eq!(model.node(ship_id).unwrap().parent, None);
        assert_eq!(model.children(ship_id), &[hull_id]);
        model.set_caret(Some(OutlinerNodeId::Entity(hull_id)));
        model.set_expanded(ship_id, true);
        let retired = world.retire_entity(hull).unwrap();
        assert_eq!(retired, hull_id);
        assert!(world.resolve(hull).is_err());
        model = model_of(&world, &model);
        assert!(model.node(hull_id).is_none());
        assert_eq!(model.caret(), None);
        assert!(model.is_expanded(ship_id));
        let reused = world.create_entity("Barrel");
        assert_eq!(reused.index, hull.index);
        assert_ne!(reused.generation, hull.generation);
        let reused_id = world.resolve(reused).unwrap();
        model = model_of(&world, &model);
        assert!(model.node(hull_id).is_none());
        assert_eq!(model.display_name(reused_id), Some("Barrel"));
        assert_ne!(model.node(reused_id).unwrap().parent, Some(ship_id));
        assert!(model.is_expanded(reused_id));
    }

    #[test]
    fn sibling_order_is_insertion_order_not_alphabetical() {
        let mut world = SceneWorld::bootstrap();
        let zebra = world.create_entity("Zebra");
        let apple = world.create_entity("apple");
        let model = model_of(&world, &WorldOutlinerModel::empty());
        let roots = model.roots();
        let zebra_at = roots.iter().position(|id| *id == world.resolve(zebra).unwrap()).unwrap();
        let apple_at = roots.iter().position(|id| *id == world.resolve(apple).unwrap()).unwrap();
        assert!(zebra_at < apple_at);
    }

    #[test]
    fn expansion_is_editor_state_and_collapsing_world_is_not_a_scene_edit() {
        let world = SceneWorld::bootstrap();
        let mut model = model_of(&world, &WorldOutlinerModel::empty());
        let revision = world.revision();
        let near = model.roots()[0];
        model.set_expanded(near, false);
        model.set_world_expanded(false);
        model.set_caret(Some(OutlinerNodeId::WorldRoot));
        let again = model_of(&world, &model);
        assert_eq!(world.revision(), revision);
        assert!(!again.is_expanded(near));
        assert!(!again.world_expanded());
        assert_eq!(again.caret(), Some(OutlinerNodeId::WorldRoot));
        assert_eq!(again.entity_count(), world.entity_count());
    }

    #[test]
    fn folders_group_world_roots_and_leave_spatial_parents_alone() {
        let mut world = SceneWorld::bootstrap();
        let crate_entity = world.create_entity("Crate");
        let crate_id = world.resolve(crate_entity).unwrap();
        let environment = world.create_scene_folder("Environment", None).unwrap();
        let landscape = world.create_scene_folder("Landscape", Some(environment)).unwrap();
        let gameplay = world.create_scene_folder("Gameplay", None).unwrap();
        world.move_entity_to_folder(crate_id, Some(landscape)).unwrap();
        let ship = world.create_entity("Ship");
        let hull = world.create_entity("Hull");
        let ship_id = world.resolve(ship).unwrap();
        let hull_id = world.resolve(hull).unwrap();
        world.reparent_entity(hull, Some(ship)).unwrap();
        assert!(world.move_entity_to_folder(hull_id, Some(environment)).is_err());
        let mut model = model_of(&world, &WorldOutlinerModel::empty());
        assert!(model.roots().contains(&crate_id));
        assert!(model.node(crate_id).unwrap().parent.is_none());
        model.place_folders(world.organization());
        assert!(!model.roots().contains(&crate_id));
        assert_eq!(model.folder_roots(), &[environment, gameplay]);
        assert_eq!(model.folder(environment).unwrap().child_folders, vec![landscape]);
        assert_eq!(model.folder(landscape).unwrap().entities, vec![crate_id]);
        assert!(model.folder(gameplay).unwrap().entities.is_empty());
        assert!(model.node(crate_id).unwrap().parent.is_none());
        assert_eq!(model.children(ship_id), &[hull_id]);
        assert!(model.folder(environment).unwrap().entities.iter().all(|id| *id != hull_id));
        assert!(model.is_folder_expanded(environment));
        model.apply_folder_expansion(&[environment, landscape, gameplay], &[landscape, gameplay]);
        assert!(!model.is_folder_expanded(environment));
        assert!(model.is_folder_expanded(landscape));
        assert_eq!(world.revision(), model.revision());
        model.set_folder_expanded(environment, true);
        model.set_folder_expanded(environment, false);
        let mut again = model_of(&world, &model);
        again.place_folders(world.organization());
        assert!(model.same_rows(&again));
        assert!(!again.is_folder_expanded(environment));
        world.rename_scene_folder(landscape, "Structures").unwrap();
        let mut renamed = model_of(&world, &again);
        renamed.place_folders(world.organization());
        assert!(!again.same_rows(&renamed));
        assert_eq!(renamed.folder(landscape).unwrap().name, "Structures");
        world.move_entity_to_folder(crate_id, None).unwrap();
        let mut back = model_of(&world, &renamed);
        back.place_folders(world.organization());
        assert!(!renamed.same_rows(&back));
        assert!(back.roots().contains(&crate_id));
        assert!(back.folder(landscape).unwrap().entities.is_empty());
        assert!(back.node(crate_id).unwrap().parent.is_none());
    }
}

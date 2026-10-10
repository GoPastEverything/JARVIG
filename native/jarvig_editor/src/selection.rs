//! Editor selection. One service for every panel.
//!
//! This is editor-session state for the active authoring world. It is not a
//! component, not a scene file, and not an engine command. The editor thread
//! owns it. Another world or a play-in-editor copy must not share this value
//! just because a uuid's bytes match. Closing that document should reconcile
//! or clear selection. There is no world uuid yet.
//!
//! Entity rows use [`EntityUuid`]. [`EntityHandle`](jarvig_core::EntityHandle)
//! is resolved only when a later command needs the live slot. A reused slot
//! does not become selected.
//!
//! Order is explicit selection order. The last item is primary: the most
//! recently explicitly selected survivor. `add` of an item that is already
//! selected does not move it. A plain click uses `replace`. Shift-range is
//! not implemented. It depends on outliner presentation order.
//!
//! Undo does not record selection. A future UI history might. Scene save does
//! not. JRV-0069 may remember the last selection with the workspace, not with
//! the world.

use jarvig_core::EntityUuid;

/// What changed the selection. Diagnostic only. Not identity, and not a vote
/// about which panel is more authoritative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SelectionSource {
    Outliner,
    Viewport,
    Inspector,
    Command,
    Programmatic,
}

/// Editor selection identity. Only entities exist today.
///
/// Later variants can name an asset, a component, a material, or a subobject.
/// Those ids are not invented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionItem {
    Entity(EntityUuid),
}

impl SelectionItem {
    pub fn entity(id: EntityUuid) -> Result<Self, SelectionError> {
        if id.is_persistent() {
            Ok(Self::Entity(id))
        } else {
            Err(SelectionError::Invalid)
        }
    }

    pub fn entity_id(self) -> Option<EntityUuid> {
        match self {
            Self::Entity(id) => Some(id),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionError {
    Invalid,
}

impl std::fmt::Display for SelectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("selection item is not a persistent entity")
    }
}

/// Ordered unique selection. Primary is the last item.
#[derive(Clone, Debug, Default)]
pub struct SelectionService {
    items: Vec<SelectionItem>,
    revision: u64,
    last_source: Option<SelectionSource>,
}

impl SelectionService {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn clear(&mut self) {
        self.clear_from(SelectionSource::Programmatic);
    }

    pub fn clear_from(&mut self, source: SelectionSource) {
        self.commit(Vec::new(), Some(source));
    }

    pub fn replace(&mut self, item: SelectionItem) -> Result<(), SelectionError> {
        self.replace_from(item, SelectionSource::Programmatic)
    }

    pub fn replace_from(&mut self, item: SelectionItem, source: SelectionSource) -> Result<(), SelectionError> {
        let item = Self::valid(item)?;
        self.commit(vec![item], Some(source));
        Ok(())
    }

    pub fn replace_many(&mut self, items: &[SelectionItem]) -> Result<(), SelectionError> {
        self.replace_many_from(items, SelectionSource::Programmatic)
    }

    pub fn replace_many_from(&mut self, items: &[SelectionItem], source: SelectionSource) -> Result<(), SelectionError> {
        let mut next = Vec::with_capacity(items.len());
        for item in items {
            let item = Self::valid(*item)?;
            if let Some(index) = next.iter().position(|stored| *stored == item) {
                next.remove(index);
            }
            next.push(item);
        }
        self.commit(next, Some(source));
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn add(&mut self, item: SelectionItem) -> Result<(), SelectionError> {
        self.add_from(item, SelectionSource::Programmatic)
    }

    pub fn add_from(&mut self, item: SelectionItem, source: SelectionSource) -> Result<(), SelectionError> {
        let item = Self::valid(item)?;
        if self.items.contains(&item) {
            return Ok(());
        }
        let mut next = self.items.clone();
        next.push(item);
        self.commit(next, Some(source));
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn remove(&mut self, item: SelectionItem) -> Result<(), SelectionError> {
        self.remove_from(item, SelectionSource::Programmatic)
    }

    pub fn remove_from(&mut self, item: SelectionItem, source: SelectionSource) -> Result<(), SelectionError> {
        let item = Self::valid(item)?;
        if !self.items.contains(&item) {
            return Ok(());
        }
        let next = self.items.iter().copied().filter(|stored| *stored != item).collect();
        self.commit(next, Some(source));
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn toggle(&mut self, item: SelectionItem) -> Result<(), SelectionError> {
        self.toggle_from(item, SelectionSource::Programmatic)
    }

    pub fn toggle_from(&mut self, item: SelectionItem, source: SelectionSource) -> Result<(), SelectionError> {
        let item = Self::valid(item)?;
        if self.items.contains(&item) {
            self.remove_from(item, source)
        } else {
            self.add_from(item, source)
        }
    }

    pub fn contains(&self, item: SelectionItem) -> bool {
        self.items.contains(&item)
    }

    pub fn items(&self) -> &[SelectionItem] {
        &self.items
    }

    pub fn count(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Most recently explicitly selected item that is still in the set.
    pub fn primary(&self) -> Option<SelectionItem> {
        self.items.last().copied()
    }

    pub fn primary_entity(&self) -> Option<EntityUuid> {
        self.primary().and_then(SelectionItem::entity_id)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn last_source(&self) -> Option<SelectionSource> {
        self.last_source
    }

    /// Drop uuids that are not in the live registry. Does not select anything new.
    pub fn reconcile_entities(&mut self, live: &[EntityUuid]) {
        let next = self
            .items
            .iter()
            .copied()
            .filter(|item| match item {
                SelectionItem::Entity(id) => live.contains(id),
            })
            .collect();
        self.commit(next, None);
    }

    fn valid(item: SelectionItem) -> Result<SelectionItem, SelectionError> {
        match item {
            SelectionItem::Entity(id) if id.is_persistent() => Ok(item),
            _ => Err(SelectionError::Invalid),
        }
    }

    /// Bumps when the effective selection changes. Order and primary are part of
    /// that state, not a side detail of set membership. The operations today
    /// change the vector whenever primary or order changes. A future operation
    /// that changes only primary or order must bump this too.
    fn commit(&mut self, next: Vec<SelectionItem>, source: Option<SelectionSource>) {
        if next == self.items {
            return;
        }
        self.items = next;
        self.revision = self.revision.saturating_add(1);
        if let Some(source) = source {
            self.last_source = Some(source);
        }
    }
}

/// `None` is the outliner world root. That row is not an entity, so it clears
/// selection instead of inserting a fake id. Ctrl does not change that.
pub fn activate_outliner_row(selection: &mut SelectionService, row: Option<EntityUuid>, ctrl: bool) -> Result<(), SelectionError> {
    let Some(id) = row else {
        selection.clear_from(SelectionSource::Outliner);
        return Ok(());
    };
    let item = SelectionItem::entity(id)?;
    if ctrl {
        selection.toggle_from(item, SelectionSource::Outliner)
    } else {
        selection.replace_from(item, SelectionSource::Outliner)
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn inspector_summary(selection: &SelectionService, name_of: impl Fn(EntityUuid) -> String) -> String {
    match selection.items() {
        [] => "No selection.".to_string(),
        [SelectionItem::Entity(id)] => format!("{}\r\nEntity\r\n{id}", name_of(*id)),
        items => format!("{} entities selected", items.len()),
    }
}

/// One solid's faces, edges, or vertices. A modeling command receives this whole set.
///
/// A plain click replaces the set. Ctrl toggles one element, and Ctrl wins over Shift.
/// Shift adds. A different solid, or a different element kind, replaces the set.
/// Order is click order. The primary is the last element the click named, which can
/// stay in place when Shift names an element that is already selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionSet {
    entity: Option<EntityUuid>,
    kind: Option<ElementKind>,
    ids: Vec<u32>,
    primary: Option<u32>,
}

/// The element a solid click names. This is not an entity, and not a triangle index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementKind {
    Face,
    Edge,
    Vertex,
}

impl Default for SelectionSet {
    fn default() -> Self {
        Self { entity: None, kind: None, ids: Vec::new(), primary: None }
    }
}

impl SelectionSet {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn entity(&self) -> Option<EntityUuid> {
        self.entity
    }

    pub fn kind(&self) -> Option<ElementKind> {
        self.kind
    }

    pub fn ids(&self) -> &[u32] {
        &self.ids
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn primary(&self) -> Option<u32> {
        self.primary.filter(|id| self.ids.contains(id))
    }

    /// Replaces the set in the given order. Zero and duplicates are dropped. An empty list clears it.
    ///
    /// This is not a click and not an undo entry. The caller does not write the solid.
    pub fn replace_ids(&mut self, entity: EntityUuid, kind: ElementKind, ids: &[u32]) -> bool {
        let mut next = Vec::new();
        for id in ids {
            if *id == 0 || next.contains(id) {
                continue;
            }
            next.push(*id);
        }
        if next.is_empty() {
            let changed = !self.is_empty();
            self.clear();
            return changed;
        }
        let primary = next.last().copied();
        let unchanged = self.entity == Some(entity) && self.kind == Some(kind) && self.ids == next && self.primary == primary;
        self.entity = Some(entity);
        self.kind = Some(kind);
        self.ids = next;
        self.primary = primary;
        !unchanged
    }

    /// `true` when the set changed. A repeated plain click on the same element is unchanged.
    pub fn click(&mut self, entity: EntityUuid, kind: ElementKind, id: u32, ctrl: bool, shift: bool) -> bool {
        if id == 0 {
            return false;
        }
        let same_target = self.entity == Some(entity) && self.kind == Some(kind);
        if !same_target || (!ctrl && !shift) {
            let unchanged = same_target && self.ids.as_slice() == [id] && self.primary == Some(id);
            self.entity = Some(entity);
            self.kind = Some(kind);
            self.ids = vec![id];
            self.primary = Some(id);
            return !unchanged;
        }
        if ctrl {
            if let Some(index) = self.ids.iter().position(|stored| *stored == id) {
                self.ids.remove(index);
                self.primary = self.ids.last().copied();
                if self.ids.is_empty() {
                    self.clear();
                }
            } else {
                self.ids.push(id);
                self.primary = Some(id);
            }
            return true;
        }
        let mut changed = false;
        if !self.ids.contains(&id) {
            self.ids.push(id);
            changed = true;
        }
        if self.primary != Some(id) {
            self.primary = Some(id);
            changed = true;
        }
        changed
    }

    /// Box selection uses the same replace, toggle, and add rules as a click.
    ///
    /// A different solid or element kind replaces the set, including when Ctrl or Shift is held.
    /// Zero is dropped. An empty hit list on a plain drag clears the set. Ctrl or Shift with no hits leaves it.
    pub fn apply_box(&mut self, entity: EntityUuid, kind: ElementKind, hits: &[u32], ctrl: bool, shift: bool) -> bool {
        let mut unique = Vec::new();
        for id in hits {
            if *id != 0 && !unique.contains(id) {
                unique.push(*id);
            }
        }
        let same_target = self.entity == Some(entity) && self.kind == Some(kind);
        if !same_target || (!ctrl && !shift) {
            return self.replace_ids(entity, kind, &unique);
        }
        if unique.is_empty() {
            return false;
        }
        if ctrl {
            for id in unique {
                if let Some(index) = self.ids.iter().position(|stored| *stored == id) {
                    self.ids.remove(index);
                } else {
                    self.ids.push(id);
                }
            }
            self.primary = self.ids.last().copied();
            if self.ids.is_empty() {
                self.clear();
            }
            return true;
        }
        let mut changed = false;
        for id in &unique {
            if !self.ids.contains(id) {
                self.ids.push(*id);
                changed = true;
            }
        }
        let primary = unique.last().copied();
        if self.primary != primary {
            self.primary = primary;
            changed = true;
        }
        changed
    }

    /// One element prints its id. Several print the count, so one id is not the selection.
    pub fn label(&self) -> Option<String> {
        let kind = self.kind?;
        let count = self.ids.len();
        if count == 0 {
            return None;
        }
        if count == 1 {
            let id = self.primary()?;
            return Some(match kind {
                ElementKind::Face => format!("F:{id}"),
                ElementKind::Edge => format!("E:{id}"),
                ElementKind::Vertex => format!("V:{id}"),
            });
        }
        let word = match kind {
            ElementKind::Face => "Faces",
            ElementKind::Edge => "Edges",
            ElementKind::Vertex => "Vertices",
        };
        Some(format!("{count} {word} selected"))
    }
}

pub fn status_selection(selection: &SelectionService, name_of: impl Fn(EntityUuid) -> String) -> String {
    match selection.items() {
        [] => "No selection".to_string(),
        [SelectionItem::Entity(id)] => format!("Selected: {}", name_of(*id)),
        items => format!("{} selected", items.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvig_core::{EntityId, SceneWorld};

    fn item(id: EntityId) -> SelectionItem {
        SelectionItem::entity(id).unwrap()
    }

    fn live(world: &SceneWorld) -> Vec<EntityUuid> {
        world.entity_outline().into_iter().map(|row| row.uuid).collect()
    }

    #[test]
    fn order_primary_and_revision_follow_explicit_edits() {
        let mut selection = SelectionService::default();
        assert!(selection.is_empty());
        assert_eq!(selection.revision(), 0);
        assert!(selection.primary().is_none());
        selection.clear();
        assert_eq!(selection.revision(), 0);
        let first = EntityId::new();
        let second = EntityId::new();
        selection.replace(item(first)).unwrap();
        let replaced = selection.revision();
        assert!(replaced > 0);
        selection.replace(item(first)).unwrap();
        assert_eq!(selection.revision(), replaced);
        selection.add(item(second)).unwrap();
        assert_eq!(selection.items(), &[item(first), item(second)]);
        assert_eq!(selection.primary_entity(), Some(second));
        let added = selection.revision();
        selection.add(item(second)).unwrap();
        assert_eq!(selection.revision(), added);
        assert_eq!(selection.items(), &[item(first), item(second)]);
        selection.toggle(item(first)).unwrap();
        assert_eq!(selection.items(), &[item(second)]);
        selection.toggle(item(first)).unwrap();
        assert_eq!(selection.items(), &[item(second), item(first)]);
        assert_eq!(selection.primary_entity(), Some(first));
        selection.toggle(item(first)).unwrap();
        selection.remove(item(second)).unwrap();
        assert!(selection.is_empty());
        let empty = selection.revision();
        selection.remove(item(second)).unwrap();
        assert_eq!(selection.revision(), empty);
        selection.replace_many(&[item(second), item(first), item(second)]).unwrap();
        assert_eq!(selection.items(), &[item(first), item(second)]);
        assert_eq!(selection.primary_entity(), Some(second));
    }

    #[test]
    fn nil_is_rejected_and_world_root_clears_without_becoming_an_item() {
        let mut selection = SelectionService::default();
        let kept = EntityId::new();
        selection.replace(item(kept)).unwrap();
        assert!(matches!(selection.replace(SelectionItem::Entity(EntityId::nil())), Err(SelectionError::Invalid)));
        assert_eq!(selection.primary_entity(), Some(kept));
        activate_outliner_row(&mut selection, None, true).unwrap();
        assert!(selection.is_empty());
        assert!(selection.primary().is_none());
        assert_eq!(selection.last_source(), Some(SelectionSource::Outliner));
        assert!(selection.items().iter().all(|stored| stored.entity_id().is_some_and(EntityId::is_persistent)));
    }

    #[test]
    fn primary_survivor_is_the_last_remaining_explicit_item() {
        let mut selection = SelectionService::default();
        let a = EntityId::new();
        let b = EntityId::new();
        let c = EntityId::new();
        selection.replace_many(&[item(a), item(b), item(c)]).unwrap();
        selection.remove(item(c)).unwrap();
        assert_eq!(selection.items(), &[item(a), item(b)]);
        assert_eq!(selection.primary_entity(), Some(b));
        selection.remove(item(b)).unwrap();
        assert_eq!(selection.primary_entity(), Some(a));
    }

    #[test]
    fn world_identity_survives_rename_and_reparent_and_ignores_slot_reuse() {
        let mut world = SceneWorld::bootstrap();
        let outline = world.entity_outline();
        let near = outline[0].uuid;
        let far = outline[1].uuid;
        let mut selection = SelectionService::default();
        selection.replace(item(near)).unwrap();
        let revision = selection.revision();
        let near_handle = world.find_entity(near).unwrap();
        world.rename_entity(near_handle, "Player Start").unwrap();
        selection.reconcile_entities(&live(&world));
        assert_eq!(selection.revision(), revision);
        assert_eq!(selection.primary_entity(), Some(near));
        assert_eq!(inspector_summary(&selection, |_| "Player Start".into()), format!("Player Start\r\nEntity\r\n{near}"));
        let far_handle = world.find_entity(far).unwrap();
        world.reparent_entity(near_handle, Some(far_handle)).unwrap();
        selection.reconcile_entities(&live(&world));
        assert_eq!(selection.revision(), revision);
        let copy = world.duplicate_entity(near_handle).unwrap();
        let copy_id = world.resolve(copy).unwrap();
        selection.reconcile_entities(&live(&world));
        assert_eq!(selection.primary_entity(), Some(near));
        assert!(!selection.contains(item(copy_id)));
        world.retire_entity(copy).unwrap();
        let a = world.create_entity("A");
        let b = world.create_entity("B");
        let c = world.create_entity("C");
        let a_id = world.resolve(a).unwrap();
        let b_id = world.resolve(b).unwrap();
        let c_id = world.resolve(c).unwrap();
        selection.replace_many(&[item(a_id), item(b_id), item(c_id)]).unwrap();
        assert_eq!(selection.primary_entity(), Some(c_id));
        world.retire_entity(b).unwrap();
        selection.reconcile_entities(&live(&world));
        assert_eq!(selection.items(), &[item(a_id), item(c_id)]);
        assert_eq!(selection.primary_entity(), Some(c_id));
        assert!(!selection.contains(item(b_id)));
        let reused = world.create_entity("D");
        assert_eq!(reused.index, b.index);
        assert_ne!(reused.generation, b.generation);
        let reused_id = world.resolve(reused).unwrap();
        selection.reconcile_entities(&live(&world));
        assert!(!selection.contains(item(reused_id)));
        assert!(!selection.contains(item(b_id)));
        assert_eq!(inspector_summary(&selection, |_| "x".into()), "2 entities selected");
        world.retire_entity(a).unwrap();
        world.retire_entity(c).unwrap();
        world.retire_entity(reused).unwrap();
        selection.reconcile_entities(&live(&world));
        assert!(selection.is_empty());
        assert_eq!(inspector_summary(&selection, |_| "x".into()), "No selection.");
        assert_eq!(status_selection(&selection, |_| "x".into()), "No selection");
    }

    fn solid() -> EntityUuid {
        EntityId::new()
    }

    #[test]
    fn ctrl_selects_two_disconnected_edges_and_a_plain_click_replaces_them() {
        let mut set = SelectionSet::default();
        let entity = solid();
        // Canonical box edges 16 (vertices 6–8) and 13 (vertices 1–3) share no vertex.
        assert!(set.click(entity, ElementKind::Edge, 16, false, false));
        assert!(set.click(entity, ElementKind::Edge, 13, true, false));
        assert_eq!(set.ids(), &[16, 13]);
        assert_eq!(set.primary(), Some(13));
        assert_eq!(set.label().as_deref(), Some("2 Edges selected"));
        assert!(set.click(entity, ElementKind::Edge, 13, true, true));
        assert_eq!(set.ids(), &[16]);
        assert_eq!(set.label().as_deref(), Some("E:16"));
        assert!(!set.click(entity, ElementKind::Edge, 16, false, false));
        set.clear();
        assert!(set.is_empty());
        assert!(set.label().is_none());
    }

    #[test]
    fn ctrl_selects_two_edges_that_share_a_corner() {
        let mut set = SelectionSet::default();
        let entity = solid();
        // Canonical box edges 16 (6–8) and 12 (7–8) meet at vertex 8.
        set.click(entity, ElementKind::Edge, 16, false, false);
        set.click(entity, ElementKind::Edge, 12, true, false);
        assert_eq!(set.ids(), &[16, 12]);
        assert_eq!(set.kind(), Some(ElementKind::Edge));
        assert_eq!(set.label().as_deref(), Some("2 Edges selected"));
        let other = solid();
        set.click(other, ElementKind::Edge, 12, true, false);
        assert_eq!(set.entity(), Some(other));
        assert_eq!(set.ids(), &[12]);
    }

    #[test]
    fn ctrl_toggles_faces_and_vertices_and_a_kind_change_replaces_the_set() {
        let mut set = SelectionSet::default();
        let entity = solid();
        set.click(entity, ElementKind::Face, 3, false, false);
        set.click(entity, ElementKind::Face, 7, true, false);
        set.click(entity, ElementKind::Face, 8, false, true);
        assert_eq!(set.ids(), &[3, 7, 8]);
        assert_eq!(set.label().as_deref(), Some("3 Faces selected"));
        set.click(entity, ElementKind::Vertex, 4, true, false);
        assert_eq!(set.kind(), Some(ElementKind::Vertex));
        assert_eq!(set.ids(), &[4]);
        set.click(entity, ElementKind::Vertex, 4, true, false);
        assert!(set.is_empty());
    }

    #[test]
    fn replace_ids_keeps_click_order_and_the_count_label() {
        let mut set = SelectionSet::default();
        let entity = solid();
        assert!(set.replace_ids(entity, ElementKind::Edge, &[16, 12, 15]));
        assert_eq!(set.ids(), &[16, 12, 15]);
        assert_eq!(set.primary(), Some(15));
        assert_eq!(set.label().as_deref(), Some("3 Edges selected"));
        assert!(set.replace_ids(entity, ElementKind::Edge, &[16, 16, 0, 12]));
        assert_eq!(set.ids(), &[16, 12]);
        assert_eq!(set.label().as_deref(), Some("2 Edges selected"));
        assert!(!set.replace_ids(entity, ElementKind::Edge, &[16, 12]));
        assert!(set.replace_ids(entity, ElementKind::Edge, &[]));
        assert!(set.is_empty());
        assert!(set.apply_box(entity, ElementKind::Face, &[3, 0, 5, 3], false, false));
        assert_eq!(set.ids(), &[3, 5]);
        assert_eq!(set.primary(), Some(5));
        assert!(set.apply_box(entity, ElementKind::Face, &[3], true, true));
        assert_eq!(set.ids(), &[5]);
        assert!(set.apply_box(entity, ElementKind::Face, &[1], false, true));
        assert_eq!(set.ids(), &[5, 1]);
        assert_eq!(set.primary(), Some(1));
        let other = solid();
        assert!(set.apply_box(other, ElementKind::Face, &[4], true, true));
        assert_eq!(set.entity(), Some(other));
        assert_eq!(set.ids(), &[4]);
        assert!(!set.apply_box(other, ElementKind::Face, &[0], true, false));
        assert_eq!(set.ids(), &[4]);
    }
}

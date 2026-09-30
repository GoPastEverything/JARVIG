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
}

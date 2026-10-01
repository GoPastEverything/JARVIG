//! Authoring identity. A UUID, not a slot, a render instance, or a mesh.
//!
//! The same text form as the TypeScript entity id. A duplicate receives a new id.
//! The registry does not know about windows or the renderer.

use std::fmt;

/// Persistent authoring id. Sixteen bytes, shown as `8-4-4-4-12` lowercase hex.
///
/// This is what save files, references, and the editor store. It is not the
/// handle used for a hot lookup.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId([u8; 16]);

/// Same value as [`EntityId`]. Named for the persistent side of the registry.
pub type EntityUuid = EntityId;

/// Process-local entity lookup. Index plus generation. Not serialized.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EntityHandle {
    pub index: u32,
    pub generation: u32,
}

impl EntityHandle {
    pub const INVALID: Self = Self { index: u32::MAX, generation: 0 };
}

impl EntityId {
    /// All-zero id. Version 0, so [`Self::parse`] and [`Self::is_persistent`] reject it.
    /// It is not the outliner world root and it is not a missing parent.
    pub fn nil() -> Self {
        Self([0; 16])
    }

    /// Authoring ids are versions 1 through 8 with a variant nibble of 8, 9, a, or b.
    pub fn is_persistent(self) -> bool {
        let version = self.0[6] >> 4;
        let variant = self.0[8] >> 4;
        (1..=8).contains(&version) && matches!(variant, 8 | 9 | 10 | 11)
    }

    /// UUID version 4. Not derived from [`crate::ObjectId`].
    pub fn new() -> Self {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("entropy for an entity id");
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Self(bytes)
    }

    /// Accepts the canonical form, either case. Same shape the TypeScript scene requires.
    ///
    /// Version must be 1 through 8 and the variant must be 8, 9, a, or b.
    /// The nil id is version 0, so it is rejected. It is not a stand-in for "no entity".
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 36
            || bytes[8] != b'-'
            || bytes[13] != b'-'
            || bytes[18] != b'-'
            || bytes[23] != b'-'
        {
            return None;
        }
        let version = hex(bytes[14])?;
        if !(1..=8).contains(&version) {
            return None;
        }
        let variant = hex(bytes[19])?;
        if !matches!(variant, 8 | 9 | 10 | 11) {
            return None;
        }
        let mut out = [0u8; 16];
        let mut index = 0;
        for (start, end) in [(0, 8), (9, 13), (14, 18), (19, 23), (24, 36)] {
            let mut cursor = start;
            while cursor < end {
                let high = hex(bytes[cursor])?;
                let low = hex(bytes[cursor + 1])?;
                out[index] = (high << 4) | low;
                index += 1;
                cursor += 2;
            }
        }
        Some(Self(out))
    }

    pub fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.0;
        write!(
            formatter,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
        )
    }
}

impl fmt::Debug for EntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "EntityId({self})")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntityError {
    Missing,
    Duplicate,
    Cycle,
    /// The slot was reused or the entity was retired. The uuid is not this handle.
    Stale,
}

impl fmt::Display for EntityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Missing => "entity does not exist",
            Self::Duplicate => "entity id is already registered",
            Self::Cycle => "entity parent cycle",
            Self::Stale => "entity handle is stale",
        })
    }
}

#[derive(Clone, Debug)]
struct Slot {
    generation: u32,
    occupied: bool,
    uuid: EntityUuid,
    parent: Option<EntityHandle>,
    name: String,
    membership: Vec<ComponentMembership>,
}

/// What kind of authorable object a row is. Not a subsystem id.
///
/// `Empty` is an entity with no component yet. The registry does not store this.
/// `SceneWorld` fills it when it builds an outline, because the component data
/// stays in the light, probe, mesh, and environment records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthoringClass {
    Mesh,
    DirectionalLight,
    PointLight,
    SpotLight,
    ReflectionProbe,
    Camera,
    WorldSettings,
    Terrain,
    Empty,
}

impl AuthoringClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mesh => "Mesh",
            Self::DirectionalLight | Self::PointLight | Self::SpotLight => "Light",
            Self::ReflectionProbe => "Probe",
            Self::Camera => "Camera",
            Self::WorldSettings => "World",
            Self::Terrain => "Terrain",
            Self::Empty => "",
        }
    }
}

/// One row of a hierarchy read. Built in one pass. Not a render instance.
///
/// `parent` is the parent's uuid when that handle still resolves. A raw registry
/// retire does not rewrite children. `SceneWorld` destroy does: it stores no
/// parent before the handle goes stale, so a save cannot see the old slot. A
/// reused slot does not become the parent, because the generation no longer matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityOutlineInfo {
    pub uuid: EntityUuid,
    pub name: String,
    pub parent: Option<EntityUuid>,
    pub class: AuthoringClass,
}

/// One component on an entity. Not a uuid and not a subsystem payload.
///
/// `slot` is stable for this type. Removing slot 1 does not renumber slot 2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentMembership {
    pub type_id: crate::TypeId,
    pub slot: u32,
}

/// Maps a persistent [`EntityUuid`] to a generational [`EntityHandle`].
///
/// Lookup by handle is the slot index. The uuid is not scanned on that path.
/// Names, parents, and component membership live here. Payloads do not.
#[derive(Clone, Debug, Default)]
pub struct EntityRegistry {
    slots: Vec<Slot>,
    free: Vec<u32>,
    order: Vec<EntityHandle>,
}

impl EntityRegistry {
    pub fn create(&mut self) -> EntityHandle {
        self.allocate(EntityId::new(), None, String::new()).expect("fresh entity")
    }

    pub fn insert(&mut self, id: EntityUuid, parent: Option<EntityHandle>) -> Result<EntityHandle, EntityError> {
        if self.find(id).is_ok() {
            return Err(EntityError::Duplicate);
        }
        if let Some(parent) = parent {
            self.uuid(parent)?;
        }
        self.allocate(id, parent, String::new())
    }

    pub fn duplicate(&mut self, handle: EntityHandle) -> Result<EntityHandle, EntityError> {
        let parent = self.parent(handle)?;
        let name = self.name(handle)?.to_string();
        let membership = self.membership(handle)?.to_vec();
        let created = self.allocate(EntityId::new(), parent, name)?;
        self.slot_mut(created)?.membership = membership;
        Ok(created)
    }

    pub fn retire(&mut self, handle: EntityHandle) -> Result<EntityUuid, EntityError> {
        let uuid = self.uuid(handle)?;
        let slot = &mut self.slots[handle.index as usize];
        slot.occupied = false;
        slot.parent = None;
        slot.name.clear();
        slot.membership.clear();
        slot.generation = slot.generation.wrapping_add(1);
        if slot.generation == 0 {
            slot.generation = 1;
        }
        self.order.retain(|stored| *stored != handle);
        self.free.push(handle.index);
        Ok(uuid)
    }

    /// Hot lookup. One slot read. Does not search by uuid.
    pub fn uuid(&self, handle: EntityHandle) -> Result<EntityUuid, EntityError> {
        let slot = self.slots.get(handle.index as usize).ok_or(EntityError::Stale)?;
        if !slot.occupied || slot.generation != handle.generation {
            return Err(EntityError::Stale);
        }
        Ok(slot.uuid)
    }

    pub fn find(&self, id: EntityUuid) -> Result<EntityHandle, EntityError> {
        self.order.iter().copied().find(|handle| self.uuid(*handle).ok() == Some(id)).ok_or(EntityError::Missing)
    }

    pub fn set_parent(&mut self, handle: EntityHandle, parent: Option<EntityHandle>) -> Result<(), EntityError> {
        self.uuid(handle)?;
        if let Some(parent) = parent {
            self.uuid(parent)?;
            let mut cursor = Some(parent);
            while let Some(current) = cursor {
                if current == handle {
                    return Err(EntityError::Cycle);
                }
                cursor = self.parent(current)?;
            }
        }
        self.slot_mut(handle)?.parent = parent;
        Ok(())
    }

    pub fn parent(&self, handle: EntityHandle) -> Result<Option<EntityHandle>, EntityError> {
        Ok(self.slot(handle)?.parent)
    }

    pub fn set_name(&mut self, handle: EntityHandle, name: &str) -> Result<(), EntityError> {
        self.slot_mut(handle)?.name = name.to_string();
        Ok(())
    }

    pub fn name(&self, handle: EntityHandle) -> Result<&str, EntityError> {
        Ok(self.slot(handle)?.name.as_str())
    }

    pub fn membership(&self, handle: EntityHandle) -> Result<&[ComponentMembership], EntityError> {
        Ok(self.slot(handle)?.membership.as_slice())
    }

    /// Appends a membership entry. The same type and slot twice is [`EntityError::Duplicate`].
    pub fn add_membership(&mut self, handle: EntityHandle, type_id: crate::TypeId, slot: u32) -> Result<(), EntityError> {
        let membership = &mut self.slot_mut(handle)?.membership;
        if membership.iter().any(|entry| entry.type_id == type_id && entry.slot == slot) {
            return Err(EntityError::Duplicate);
        }
        membership.push(ComponentMembership { type_id, slot });
        Ok(())
    }

    pub fn remove_membership(&mut self, handle: EntityHandle, type_id: crate::TypeId, slot: u32) -> Result<(), EntityError> {
        let membership = &mut self.slot_mut(handle)?.membership;
        let index = membership.iter().position(|entry| entry.type_id == type_id && entry.slot == slot).ok_or(EntityError::Missing)?;
        membership.remove(index);
        Ok(())
    }

    pub fn contains(&self, id: EntityUuid) -> bool {
        self.find(id).is_ok()
    }

    pub fn iter(&self) -> impl Iterator<Item = EntityUuid> + '_ {
        self.order.iter().copied().filter_map(|handle| self.uuid(handle).ok())
    }

    pub fn handles(&self) -> impl Iterator<Item = EntityHandle> + '_ {
        self.order.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Hierarchy read. One slot read per entity, plus one for the parent.
    /// Does not scan uuids to paint a row, and does not follow a bootstrap drawable slot.
    pub fn outline(&self) -> Vec<EntityOutlineInfo> {
        self.order
            .iter()
            .copied()
            .filter_map(|handle| {
                let slot = self.slot(handle).ok()?;
                let parent = slot.parent.and_then(|parent| self.uuid(parent).ok());
                Some(EntityOutlineInfo { uuid: slot.uuid, name: slot.name.clone(), parent, class: AuthoringClass::Empty })
            })
            .collect()
    }

    fn allocate(&mut self, uuid: EntityUuid, parent: Option<EntityHandle>, name: String) -> Result<EntityHandle, EntityError> {
        if let Some(parent) = parent {
            self.uuid(parent)?;
        }
        let handle = if let Some(index) = self.free.pop() {
            let generation = self.slots[index as usize].generation;
            self.slots[index as usize] = Slot { generation, occupied: true, uuid, parent, name, membership: Vec::new() };
            EntityHandle { index, generation }
        } else {
            let index = self.slots.len() as u32;
            self.slots.push(Slot { generation: 1, occupied: true, uuid, parent, name, membership: Vec::new() });
            EntityHandle { index, generation: 1 }
        };
        self.order.push(handle);
        Ok(handle)
    }

    fn slot(&self, handle: EntityHandle) -> Result<&Slot, EntityError> {
        let slot = self.slots.get(handle.index as usize).ok_or(EntityError::Stale)?;
        if !slot.occupied || slot.generation != handle.generation {
            return Err(EntityError::Stale);
        }
        Ok(slot)
    }

    fn slot_mut(&mut self, handle: EntityHandle) -> Result<&mut Slot, EntityError> {
        let slot = self.slots.get_mut(handle.index as usize).ok_or(EntityError::Stale)?;
        if !slot.occupied || slot.generation != handle.generation {
            return Err(EntityError::Stale);
        }
        Ok(slot)
    }
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_form_round_trips_and_rejects_a_slot_number() {
        let id = EntityId::new();
        let text = id.to_string();
        assert_eq!(text.len(), 36);
        assert_eq!(EntityId::parse(&text), Some(id));
        assert_eq!(EntityId::parse(&text.to_ascii_uppercase()), Some(id));
        assert!(EntityId::parse("1").is_none());
        assert!(EntityId::parse("object-1").is_none());
        assert!(EntityId::parse("00000000-0000-0000-0000-000000000000").is_none());
        assert!(!EntityId::nil().is_persistent());
        assert!(id.is_persistent());
        assert!(EntityId::parse(&format!("zzzzzzzz{}", &text[8..])).is_none());
        assert_eq!(hex_nibble(text.as_bytes()[14]), 4);
    }

    #[test]
    fn a_duplicate_is_a_new_id_and_a_repeat_is_rejected() {
        let mut registry = EntityRegistry::default();
        let first = registry.create();
        let child_uuid = EntityId::new();
        let child = registry.insert(child_uuid, Some(first)).unwrap();
        let copy = registry.duplicate(child).unwrap();
        assert_ne!(registry.uuid(copy).unwrap(), registry.uuid(child).unwrap());
        assert_eq!(registry.parent(copy).unwrap(), Some(first));
        assert!(registry.contains(child_uuid));
        assert_eq!(registry.insert(child_uuid, None).unwrap_err(), EntityError::Duplicate);
        assert_eq!(registry.set_parent(first, Some(child)).unwrap_err(), EntityError::Cycle);
        assert_eq!(registry.iter().count(), 3);
    }

    #[test]
    fn a_retired_handle_is_stale_and_the_uuid_is_not_the_slot() {
        let mut registry = EntityRegistry::default();
        let handle = registry.create();
        registry.set_name(handle, "Near").unwrap();
        let uuid = registry.uuid(handle).unwrap();
        assert_eq!(registry.find(uuid).unwrap(), handle);
        assert_eq!(registry.name(handle).unwrap(), "Near");
        assert_ne!(uuid.to_string(), handle.index.to_string());
        let retired = registry.retire(handle).unwrap();
        assert_eq!(retired, uuid);
        assert_eq!(registry.uuid(handle).unwrap_err(), EntityError::Stale);
        assert!(registry.find(uuid).is_err());
        let reused = registry.create();
        assert_eq!(reused.index, handle.index);
        assert_ne!(reused.generation, handle.generation);
        assert_eq!(registry.uuid(handle).unwrap_err(), EntityError::Stale);
        assert!(registry.uuid(reused).is_ok());
    }

    #[test]
    fn a_retired_parent_is_not_replaced_by_the_reused_slot() {
        let mut registry = EntityRegistry::default();
        let parent = registry.create();
        registry.set_name(parent, "Ship").unwrap();
        let child = registry.create();
        registry.set_name(child, "Hull").unwrap();
        registry.set_parent(child, Some(parent)).unwrap();
        let parent_uuid = registry.uuid(parent).unwrap();
        let child_uuid = registry.uuid(child).unwrap();
        registry.retire(parent).unwrap();
        assert_eq!(registry.parent(child).unwrap(), Some(parent));
        let outline = registry.outline();
        assert!(outline.iter().all(|row| row.uuid != parent_uuid));
        let child_row = outline.iter().find(|row| row.uuid == child_uuid).unwrap();
        assert_eq!(child_row.parent, None);
        assert_eq!(child_row.name, "Hull");
        let reused = registry.create();
        assert_eq!(reused.index, parent.index);
        assert_ne!(reused.generation, parent.generation);
        registry.set_name(reused, "Other").unwrap();
        let reused_uuid = registry.uuid(reused).unwrap();
        let outline = registry.outline();
        assert!(outline.iter().all(|row| row.uuid != parent_uuid));
        let child_row = outline.iter().find(|row| row.uuid == child_uuid).unwrap();
        assert_eq!(child_row.parent, None);
        assert_ne!(Some(reused_uuid), child_row.parent);
    }

    fn hex_nibble(byte: u8) -> u8 {
        hex(byte).unwrap()
    }
}

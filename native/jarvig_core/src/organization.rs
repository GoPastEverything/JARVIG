//! Editor folders for a level. Not a geometric parent and not a surface group.
//!
//! A folder has a stable id, a visible name, and an optional parent folder.
//! Membership maps an entity uuid onto a folder. Lock is a list of uuids.
//! Neither one writes a transform, a body, a material, or an intent step.
//! ADR-0041 still owns spatial parents. Deleting a folder does not delete entities.

use crate::json_lite::Json;
use crate::EntityId;

/// Same bound as a surface-group name. The id is the identity. The name is the label.
pub const FOLDER_NAME_LIMIT: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneFolder {
    pub id: u32,
    pub name: String,
    pub parent: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FolderMember {
    entity: EntityId,
    folder: u32,
}

/// Omitted from a level when it has no folders, no members, no locks, and the next id is still 1.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SceneOrganization {
    folders: Vec<SceneFolder>,
    members: Vec<FolderMember>,
    locked: Vec<EntityId>,
    next_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrganizationError {
    EmptyName,
    NameLimit,
    UnknownFolder,
    MissingParent,
    Cycle,
    SpatialParent,
    NotFound,
}

impl std::fmt::Display for OrganizationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::EmptyName => "folder name is empty",
            Self::NameLimit => "folder name is too long",
            Self::UnknownFolder => "folder does not exist",
            Self::MissingParent => "parent folder does not exist",
            Self::Cycle => "folder parent cycle",
            Self::SpatialParent => "entity has a geometric parent",
            Self::NotFound => "entity does not exist",
        })
    }
}

impl SceneOrganization {
    pub fn is_empty(&self) -> bool {
        self.folders.is_empty() && self.members.is_empty() && self.locked.is_empty() && self.next_id <= 1
    }

    pub fn folders(&self) -> &[SceneFolder] {
        &self.folders
    }

    pub fn folder(&self, id: u32) -> Option<&SceneFolder> {
        self.folders.iter().find(|folder| folder.id == id)
    }

    pub fn members(&self) -> Vec<(EntityId, u32)> {
        self.members.iter().map(|member| (member.entity, member.folder)).collect()
    }

    pub fn folder_of(&self, entity: EntityId) -> Option<u32> {
        self.members.iter().find(|member| member.entity == entity).map(|member| member.folder)
    }

    pub fn is_locked(&self, entity: EntityId) -> bool {
        self.locked.contains(&entity)
    }

    pub fn next_id(&self) -> u32 {
        self.next_id.max(1)
    }

    pub fn create_folder(&mut self, name: &str, parent: Option<u32>) -> Result<u32, OrganizationError> {
        let name = clean_name(name)?;
        if let Some(parent) = parent {
            if self.folder(parent).is_none() {
                return Err(OrganizationError::MissingParent);
            }
        }
        let id = self.allocate_id()?;
        self.folders.push(SceneFolder { id, name, parent });
        Ok(id)
    }

    pub fn rename_folder(&mut self, id: u32, name: &str) -> Result<bool, OrganizationError> {
        let name = clean_name(name)?;
        let folder = self.folders.iter_mut().find(|folder| folder.id == id).ok_or(OrganizationError::UnknownFolder)?;
        if folder.name == name {
            return Ok(false);
        }
        folder.name = name;
        Ok(true)
    }

    /// Removes the folder. Child folders and members move to its parent, or to World when it had none.
    /// Entities stay. The id is not reused.
    pub fn delete_folder(&mut self, id: u32) -> Result<(), OrganizationError> {
        let index = self.folders.iter().position(|folder| folder.id == id).ok_or(OrganizationError::UnknownFolder)?;
        let parent = self.folders[index].parent;
        self.folders.remove(index);
        for folder in &mut self.folders {
            if folder.parent == Some(id) {
                folder.parent = parent;
            }
        }
        if let Some(parent) = parent {
            for member in &mut self.members {
                if member.folder == id {
                    member.folder = parent;
                }
            }
        } else {
            self.members.retain(|member| member.folder != id);
        }
        Ok(())
    }

    /// `None` puts the entity back at World. A missing folder writes nothing.
    pub fn set_member(&mut self, entity: EntityId, folder: Option<u32>) -> Result<bool, OrganizationError> {
        if let Some(folder) = folder {
            if self.folder(folder).is_none() {
                return Err(OrganizationError::UnknownFolder);
            }
        }
        let current = self.folder_of(entity);
        if current == folder {
            return Ok(false);
        }
        self.members.retain(|member| member.entity != entity);
        if let Some(folder) = folder {
            self.members.push(FolderMember { entity, folder });
        }
        Ok(true)
    }

    pub fn set_locked(&mut self, entity: EntityId, locked: bool) -> bool {
        let present = self.locked.contains(&entity);
        if present == locked {
            return false;
        }
        if locked {
            self.locked.push(entity);
        } else {
            self.locked.retain(|stored| *stored != entity);
        }
        true
    }

    /// Drops a destroyed entity. Folders stay.
    pub fn forget(&mut self, entity: EntityId) {
        self.members.retain(|member| member.entity != entity);
        self.locked.retain(|stored| *stored != entity);
    }

    /// Load keeps empty folders. Members and locks whose entity is gone are dropped.
    pub fn retained(mut self, live: &[EntityId]) -> Self {
        self.members.retain(|member| live.contains(&member.entity));
        self.locked.retain(|entity| live.contains(entity));
        self
    }

    pub fn validate(&self) -> Result<(), OrganizationError> {
        let mut seen = Vec::new();
        for folder in &self.folders {
            if folder.id == 0 || seen.contains(&folder.id) {
                return Err(OrganizationError::UnknownFolder);
            }
            seen.push(folder.id);
            if folder.name.is_empty() || folder.name.chars().count() > FOLDER_NAME_LIMIT || folder.name.chars().any(|glyph| glyph.is_control()) {
                return Err(OrganizationError::NameLimit);
            }
            if let Some(parent) = folder.parent {
                if parent == folder.id || self.folder(parent).is_none() {
                    return Err(OrganizationError::MissingParent);
                }
            }
            let mut cursor = folder.parent;
            let mut guard = self.folders.len() + 1;
            while let Some(parent) = cursor {
                guard -= 1;
                if guard == 0 || parent == folder.id {
                    return Err(OrganizationError::Cycle);
                }
                cursor = self.folder(parent).and_then(|folder| folder.parent);
            }
            if self.next_id() <= folder.id {
                return Err(OrganizationError::UnknownFolder);
            }
        }
        let mut members = Vec::new();
        for member in &self.members {
            if members.contains(&member.entity) || self.folder(member.folder).is_none() {
                return Err(OrganizationError::UnknownFolder);
            }
            members.push(member.entity);
        }
        let mut locks = Vec::new();
        for entity in &self.locked {
            if locks.contains(entity) {
                return Err(OrganizationError::UnknownFolder);
            }
            locks.push(*entity);
        }
        Ok(())
    }

    pub fn to_json(&self) -> Json {
        let folders = Json::array(
            self.folders
                .iter()
                .map(|folder| {
                    let mut fields = vec![("id", Json::int(i64::from(folder.id))), ("name", Json::string(&folder.name))];
                    if let Some(parent) = folder.parent {
                        fields.push(("parent", Json::int(i64::from(parent))));
                    }
                    Json::object(fields)
                })
                .collect(),
        );
        let mut fields = vec![("folders", folders)];
        if !self.members.is_empty() {
            fields.push((
                "members",
                Json::array(
                    self.members
                        .iter()
                        .map(|member| {
                            Json::object(vec![
                                ("entity", Json::string(member.entity.to_string())),
                                ("folder", Json::int(i64::from(member.folder))),
                            ])
                        })
                        .collect(),
                ),
            ));
        }
        if !self.locked.is_empty() {
            fields.push(("locked", Json::array(self.locked.iter().map(|entity| Json::string(entity.to_string())).collect())));
        }
        if self.next_id() != 1 {
            fields.push(("next_folder", Json::int(i64::from(self.next_id()))));
        }
        Json::object(fields)
    }

    pub fn from_json(json: &Json) -> Result<Self, String> {
        let mut organization = Self::default();
        let Some(folders) = json.get("folders").and_then(Json::as_array) else {
            return Err("organization folders are missing".into());
        };
        for folder in folders {
            let id = json_u32(folder.get("id")).ok_or_else(|| "organization folder id is invalid".to_string())?;
            let name = folder.get("name").and_then(Json::as_str).ok_or_else(|| "organization folder name is missing".to_string())?.to_string();
            let parent = match folder.get("parent") {
                None => None,
                Some(value) => Some(json_u32(Some(value)).ok_or_else(|| "organization folder parent is invalid".to_string())?),
            };
            organization.folders.push(SceneFolder { id, name, parent });
        }
        if let Some(members) = json.get("members").and_then(Json::as_array) {
            for member in members {
                let entity = member
                    .get("entity")
                    .and_then(Json::as_str)
                    .and_then(EntityId::parse)
                    .ok_or_else(|| "organization member is invalid".to_string())?;
                let folder = json_u32(member.get("folder")).ok_or_else(|| "organization member folder is invalid".to_string())?;
                organization.members.push(FolderMember { entity, folder });
            }
        }
        if let Some(locked) = json.get("locked").and_then(Json::as_array) {
            for entity in locked {
                let entity = entity.as_str().and_then(EntityId::parse).ok_or_else(|| "organization lock is invalid".to_string())?;
                organization.locked.push(entity);
            }
        }
        organization.next_id = match json.get("next_folder") {
            None => 1,
            Some(value) => json_u32(Some(value)).ok_or_else(|| "organization next folder is invalid".to_string())?,
        };
        if organization.next_id == 0 {
            return Err("organization next folder is invalid".into());
        }
        let issued = organization.folders.iter().map(|folder| folder.id).max().unwrap_or(0);
        if organization.next_id <= issued {
            organization.next_id = issued.saturating_add(1);
        }
        organization.validate().map_err(|error| error.to_string())?;
        Ok(organization)
    }

    fn allocate_id(&mut self) -> Result<u32, OrganizationError> {
        let id = self.next_id.max(1);
        if id == 0 || id == u32::MAX {
            return Err(OrganizationError::UnknownFolder);
        }
        self.next_id = id.saturating_add(1);
        Ok(id)
    }
}

fn clean_name(name: &str) -> Result<String, OrganizationError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(OrganizationError::EmptyName);
    }
    if name.chars().count() > FOLDER_NAME_LIMIT || name.chars().any(|glyph| glyph.is_control()) {
        return Err(OrganizationError::NameLimit);
    }
    Ok(name.to_string())
}

fn json_u32(value: Option<&Json>) -> Option<u32> {
    let number = value?.as_f64()?;
    if number < 1.0 || number > u32::MAX as f64 {
        return None;
    }
    let id = number as u32;
    if id as f64 == number { Some(id) } else { None }
}

#[cfg(test)]
mod tests {
    use crate::{BlockRecord, EntityId, EnvironmentLight, LevelDocument, SceneWorld, Vec3};

    fn session() -> (SceneWorld, EntityId, EntityId, EntityId) {
        let mut world = SceneWorld::new_session();
        world.spawn_saved_world_settings(EntityId::new(), "World Settings", EnvironmentLight::bootstrap()).unwrap();
        let crate_id = world.create_block(Vec3::new(1.0, 1.0, -4.0), BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let lamp = world.create_block(Vec3::new(3.0, 1.0, -4.0), BlockRecord::standard([1.0, 1.0, 1.0]).unwrap()).unwrap();
        let door = world.create_block(Vec3::new(5.0, 1.0, -4.0), BlockRecord::standard([1.0, 2.0, 0.2]).unwrap()).unwrap();
        world.set_entity_name(crate_id, "Crate").unwrap();
        world.set_entity_name(lamp, "Lamp").unwrap();
        world.set_entity_name(door, "Door").unwrap();
        (world, crate_id, lamp, door)
    }

    #[test]
    fn folders_rename_and_moves_survive_reload_without_moving_the_solid() {
        let (mut world, crate_id, lamp, door) = session();
        let environment = world.create_scene_folder("Environment", None).unwrap();
        let landscape = world.create_scene_folder("Landscape", Some(environment)).unwrap();
        let architecture = world.create_scene_folder("Architecture", Some(environment)).unwrap();
        world.create_scene_folder("Gameplay", None).unwrap();
        world.create_scene_folder("Lighting", None).unwrap();
        world.create_scene_folder("Props", None).unwrap();
        world.rename_scene_folder(architecture, "Structures").unwrap();
        world.move_entity_to_folder(crate_id, Some(landscape)).unwrap();
        world.move_entity_to_folder(lamp, Some(architecture)).unwrap();
        world.create_block_surface_group(crate_id, "Upper Rim", &[3, 5]).unwrap();
        let before = world.entity_world_pose(crate_id).unwrap();
        let parent = world.entity_parent(crate_id).unwrap();
        let record = world.authored_block(crate_id).unwrap();
        let lamp_local = world.entity_local_pose(lamp).unwrap().translation;
        world.set_entity_locked(lamp, true).unwrap();
        assert!(world.set_entity_local_translation(lamp, Vec3::new(9.0, 1.0, -4.0)).is_err());
        assert!(world.destroy_authored(lamp).is_err());
        assert_eq!(world.entity_local_pose(lamp).unwrap().translation, lamp_local);
        let copy = world.duplicate_authored(crate_id).unwrap();
        assert_eq!(world.scene_folder_of(copy), None);
        assert!(world.move_entity_to_folder(door, Some(999)).is_err());
        let text = LevelDocument::capture(&world, EntityId::new(), "Organized").unwrap().to_json();
        assert!(text.contains("\"organization\""));
        assert!(text.contains("Landscape"));
        assert!(text.contains("Structures"));
        let loaded = crate::parse_level(&text).unwrap().instantiate().unwrap();
        assert_eq!(loaded.scene_folder_of(crate_id), Some(landscape));
        assert_eq!(loaded.scene_folder_of(lamp), Some(architecture));
        assert_eq!(loaded.scene_folder_of(door), None);
        assert!(loaded.entity_locked(lamp));
        assert_eq!(loaded.entity_parent(crate_id).unwrap(), parent);
        assert_eq!(loaded.entity_world_pose(crate_id).unwrap().translation, before.translation);
        assert_eq!(loaded.entity_local_pose(lamp).unwrap().translation, lamp_local);
        let loaded_record = loaded.authored_block(crate_id).unwrap();
        assert_eq!(loaded_record.body, record.body);
        assert_eq!(loaded_record.materials, record.materials);
        assert_eq!(loaded_record.face_materials, record.face_materials);
        assert_eq!(loaded_record.surface_groups.len(), 1);
        assert_eq!(loaded_record.surface_groups[0].name, "Upper Rim");
        assert!(loaded_record.surface_groups[0].members.iter().any(|member| member.face == Some(3)));
        assert!(loaded_record.surface_groups[0].members.iter().any(|member| member.face == Some(5)));
    }

    #[test]
    fn a_plain_level_omits_organization_and_a_deleted_folder_does_not_reuse_its_id() {
        let plain = crate::empty_world_level().to_json();
        assert!(!plain.contains("organization"));
        let (mut world, crate_id, _, _) = session();
        let props = world.create_scene_folder("Props", None).unwrap();
        world.move_entity_to_folder(crate_id, Some(props)).unwrap();
        world.delete_scene_folder(props).unwrap();
        assert_eq!(world.scene_folder_of(crate_id), None);
        assert!(world.authored_block(crate_id).is_some());
        let next = world.create_scene_folder("Props", None).unwrap();
        assert_ne!(next, props);
        let text = LevelDocument::capture(&world, EntityId::new(), "Kept").unwrap().to_json();
        assert!(text.contains("next_folder"));
        let (mut world, parent_id, child, _) = session();
        world.reparent_authored(child, Some(parent_id)).unwrap();
        let folder = world.create_scene_folder("Environment", None).unwrap();
        let pose = world.entity_world_pose(child).unwrap().translation;
        assert!(world.move_entity_to_folder(child, Some(folder)).is_err());
        assert_eq!(world.entity_parent(child).unwrap(), Some(parent_id));
        assert_eq!(world.entity_world_pose(child).unwrap().translation, pose);
        assert_eq!(world.scene_folder_of(child), None);
    }
}

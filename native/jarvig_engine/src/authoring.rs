//! Engine-owned authoring commands. The editor, a future CLI, and automation call this.
//!
//! A command sets absolute state. It does not write a GPU buffer. Undo is not
//! implemented. The editor gizmo keeps the original value for the drag and sends
//! absolute `SetProperty` values on each update. Commit leaves the last value.
//! Cancel sends the original again. That session is not an undo stack.

use jarvig_core::{
    find_field, AuthoringError, AuthoringResult, EntityId, FieldId, PropertyValue, TypeId, FIELD_CASCADE_COUNT, FIELD_CASCADE_DISTRIBUTION,
    FIELD_CAST_SHADOWS, FIELD_COLOR, FIELD_ENABLED, FIELD_INNER_CONE, FIELD_INTENSITY, FIELD_LOCAL_ROTATION, FIELD_LOCAL_TRANSLATION, FIELD_OBJECT_SCALE,
    FIELD_LOWER_COLOR, FIELD_MATERIAL_SLOT, FIELD_NAME, FIELD_OUTER_CONE, FIELD_PRIORITY, FIELD_RADIUS, FIELD_RANGE, FIELD_RECEIVE_SHADOWS, FIELD_SHADOW_BIAS, FIELD_SURFACE,
    FIELD_METALLIC_FACTOR, FIELD_NORMAL_SCALE, FIELD_ROUGHNESS_FACTOR, FIELD_SHADOW_DISTANCE, FIELD_SHADOW_FILTER, FIELD_SHADOW_NORMAL_BIAS,
    FIELD_SHADOW_RESOLUTION, FIELD_SHADOW_SLOPE_BIAS, FIELD_UPPER_COLOR, FIELD_UV_SCALE,
    FIELD_FAR_PLANE, FIELD_NEAR_PLANE, FIELD_ORTHO_HEIGHT, FIELD_PARENT, FIELD_PROJECTION, FIELD_RESOLUTION, FIELD_UPDATE_MODE, FIELD_VERTICAL_FOV, FIELD_VIEWPORT, FIELD_VISIBLE,
    TYPE_CAMERA, TYPE_DIRECTIONAL_LIGHT, TYPE_ENTITY, TYPE_ENVIRONMENT, TYPE_JOINT, TYPE_MESH_RENDERER, TYPE_PARAMETRIC_BLOCK, TYPE_PLAYER_START, TYPE_POINT_LIGHT,
    TYPE_REFLECTION_PROBE, TYPE_SPATIAL_FRAME, TYPE_SPOT_LIGHT, TYPE_TERRAIN,
};

use crate::EngineSession;

/// Desired property state. The target is the persistent entity id.
#[derive(Clone, Debug, PartialEq)]
pub enum AuthoringCommand {
    SetProperty { target: EntityId, type_id: TypeId, field: FieldId, value: PropertyValue },
    /// New `EntityUuid` and a copied subsystem record. Does not select the copy.
    DuplicateEntity { target: EntityId },
    /// Removes the entity and its subsystem record. Children are reparented to World.
    DestroyEntity { target: EntityId },
    /// Adds one component the type allows. A second `One` component fails.
    AddComponent { target: EntityId, type_id: TypeId },
    /// Drops one component and its subsystem record. Slot 0 is the only slot these types use.
    RemoveComponent { target: EntityId, type_id: TypeId, slot: u32 },
    /// Flat chunked heightfield centered at a scene-local point. One terrain actor.
    CreateTerrain { local: jarvig_core::Vec3, record: jarvig_core::TerrainRecord },
    /// One parametric block. `local` is scene-local meters. Size stays on the solid, not entity scale.
    CreateBlock { local: jarvig_core::Vec3 },
    /// Same evaluated solid as [`Self::CreateBlock`], at the plane's width, minimum thickness, and depth.
    CreatePlane { local: jarvig_core::Vec3 },
    /// Absolute face push from a drag baseline. History is [`Self::CommitBlockFace`].
    PushBlockFace {
        target: EntityId,
        face: u8,
        baseline_size: [f64; 3],
        baseline_local: jarvig_core::Vec3,
        outward_m: f64,
    },
    /// One history entry when an extrude session is applied. A viewport drag commits this on mouse-up.
    CommitBlockFace { target: EntityId, face: u8, baseline_size: [f64; 3] },
    /// Live topological body. History is [`Self::CommitBlockTopology`]. `translation` is absolute.
    PreviewBlockBody { target: EntityId, body: jarvig_core::SolidBody, translation: jarvig_core::Vec3 },
    /// One log line after the body is already stored. Only a topology edit is accepted.
    CommitBlockTopology { target: EntityId, op: jarvig_core::BlockOp },
    /// Puts a topology drag back. Does not append history.
    RestoreBlockBody { target: EntityId, body: Option<jarvig_core::SolidBody>, size_m: [f64; 3], translation: jarvig_core::Vec3 },
    /// Midpoint of one edge. One undo entry.
    SplitBlockEdge { target: EntityId, edge: u32 },
    /// One extrude on an authored seed. The body stays absent. The entity shifts with the recenter.
    ExtrudeAuthoredFaces { target: EntityId, faces: Vec<u32>, delta_m: [f64; 3] },
    /// One quad becomes a grid of quads. One undo entry.
    SubdivideBlockFace { target: EntityId, face: u32, u: u32, v: u32 },
    /// Live bevel. History is [`Self::CommitBlockBevel`].
    PreviewBlockBevel { target: EntityId, meters: f64 },
    /// Live inset on one face. History is [`Self::CommitBlockInset`].
    PreviewBlockInset { target: EntityId, face: u8, meters: f64 },
    /// One bevel entry when the preview moved.
    CommitBlockBevel { target: EntityId, baseline_m: f64 },
    /// One inset entry when the preview moved.
    CommitBlockInset { target: EntityId, face: u8, baseline_m: f64 },
    /// Size returns to 2 m and the feature parameters clear. Translation stays.
    ResetBlockShape { target: EntityId },
    /// Independent copy sharing one face. Not a live instance.
    MirrorBlock { target: EntityId, axis: u8 },
    /// Lowest corner onto scene Y = 0.
    AlignBlockToGround { target: EntityId },
    /// Rounds local translation. Does not change a Move-gizmo drag.
    SnapBlock { target: EntityId, step_m: f64 },
    /// Puts a face drag back to its start. Does not append history.
    RestoreBlockDrag {
        target: EntityId,
        size_m: [f64; 3],
        inset_m: [f64; 6],
        bevel_m: f64,
        translation: jarvig_core::Vec3,
    },
    /// Writes the heightfield. Does not generate Einstein detail.
    /// `rebuild_meshes` builds the derived chunk visuals on this thread. Land Mode passes false
    /// and publishes those meshes from the job queue.
    StampTerrain {
        target: EntityId,
        brush: jarvig_core::TerrainBrush,
        local_x: f32,
        local_z: f32,
        radius: f32,
        delta: f32,
        layer: u8,
        falloff: jarvig_core::TerrainFalloff,
        flatten_to: Option<f32>,
        rebuild_meshes: bool,
    },
}

impl EngineSession {
    pub fn execute_authoring(&mut self, command: AuthoringCommand) -> Result<AuthoringResult, AuthoringError> {
        let result = self.dispatch_authoring(command);
        match &result {
            Ok(_) => self.authoring_commands = self.authoring_commands.saturating_add(1),
            Err(_) => self.authoring_failures = self.authoring_failures.saturating_add(1),
        }
        result
    }

    pub fn authoring_command_count(&self) -> u64 {
        self.authoring_commands
    }

    pub fn authoring_failure_count(&self) -> u64 {
        self.authoring_failures
    }

    fn finish_created_solid(&mut self, id: EntityId) -> Result<AuthoringResult, AuthoringError> {
        if self.runtime.profile().renders() {
            let master = self.current_material_master().map_err(|_| AuthoringError::InvalidOperation)?;
            self.bind_entity_material(id, master).map_err(|_| AuthoringError::InvalidOperation)?;
        }
        Ok(AuthoringResult::Created(id))
    }

    fn dispatch_authoring(&mut self, command: AuthoringCommand) -> Result<AuthoringResult, AuthoringError> {
        match command {
            AuthoringCommand::DuplicateEntity { target } => {
                let created = self.world.duplicate_authored(target)?;
                Ok(AuthoringResult::Duplicated(created))
            }
            AuthoringCommand::DestroyEntity { target } => {
                self.world.destroy_authored(target)?;
                Ok(AuthoringResult::Applied)
            }
            AuthoringCommand::AddComponent { target, type_id } => {
                self.world.add_component(target, type_id)?;
                Ok(AuthoringResult::Applied)
            }
            AuthoringCommand::RemoveComponent { target, type_id, slot } => {
                self.world.remove_component(target, type_id, slot)?;
                Ok(AuthoringResult::Applied)
            }
            AuthoringCommand::CreateTerrain { local, record } => {
                let master = if self.runtime.profile().renders() {
                    Some(self.current_material_master().map_err(|_| AuthoringError::InvalidOperation)?)
                } else {
                    None
                };
                self.world.create_terrain(local, record)?;
                if let Some(master) = master {
                    self.bind_new_chunk_materials(master).map_err(|_| AuthoringError::InvalidOperation)?;
                }
                Ok(AuthoringResult::Applied)
            }
            AuthoringCommand::PushBlockFace { target, face, baseline_size, baseline_local, outward_m } => {
                self.world.push_block_face(target, face, baseline_size, baseline_local, outward_m)
            }
            AuthoringCommand::CommitBlockFace { target, face, baseline_size } => self.world.commit_block_face(target, face, baseline_size),
            AuthoringCommand::PreviewBlockBevel { target, meters } => self.world.preview_block_bevel(target, meters),
            AuthoringCommand::PreviewBlockInset { target, face, meters } => self.world.preview_block_inset(target, face, meters),
            AuthoringCommand::CommitBlockBevel { target, baseline_m } => self.world.commit_block_bevel(target, baseline_m),
            AuthoringCommand::CommitBlockInset { target, face, baseline_m } => self.world.commit_block_inset(target, face, baseline_m),
            AuthoringCommand::ResetBlockShape { target } => self.world.reset_block_shape(target),
            AuthoringCommand::MirrorBlock { target, axis } => {
                let created = self.world.mirror_block(target, axis as usize)?;
                Ok(AuthoringResult::Duplicated(created))
            }
            AuthoringCommand::AlignBlockToGround { target } => self.world.align_block_to_ground(target),
            AuthoringCommand::SnapBlock { target, step_m } => self.world.snap_block_translation(target, step_m),
            AuthoringCommand::RestoreBlockDrag { target, size_m, inset_m, bevel_m, translation } => {
                self.world.restore_block_drag(target, size_m, inset_m, bevel_m, translation)
            }
            AuthoringCommand::PreviewBlockBody { target, body, translation } => self.world.preview_block_body(target, body, translation),
            AuthoringCommand::CommitBlockTopology { target, op } => self.world.commit_block_topology(target, op),
            AuthoringCommand::RestoreBlockBody { target, body, size_m, translation } => self.world.restore_block_body(target, body, size_m, translation),
            AuthoringCommand::SplitBlockEdge { target, edge } => self.world.split_block_edge(target, edge),
            AuthoringCommand::ExtrudeAuthoredFaces { target, faces, delta_m } => self.world.extrude_authored_faces(target, &faces, delta_m),
            AuthoringCommand::SubdivideBlockFace { target, face, u, v } => self.world.subdivide_block_face(target, face, u, v),
            AuthoringCommand::CreateBlock { local } => {
                let record = jarvig_core::BlockRecord::authored_seed([2.0, 2.0, 2.0]).map_err(|_| AuthoringError::InvalidValue)?;
                let id = self.world.create_block(local, record)?;
                self.finish_created_solid(id)
            }
            AuthoringCommand::CreatePlane { local } => {
                let id = self.world.create_plane(local)?;
                self.finish_created_solid(id)
            }
            AuthoringCommand::StampTerrain { target, brush, local_x, local_z, radius, delta, layer, falloff, flatten_to, rebuild_meshes } => {
                self.world.stamp_terrain(target, brush, local_x, local_z, radius, delta, layer, falloff, flatten_to, rebuild_meshes)?;
                Ok(AuthoringResult::Applied)
            }
            AuthoringCommand::SetProperty { target, type_id, field, value } => {
                let info = find_field(type_id, field).ok_or(AuthoringError::FieldNotFound)?;
                if !info.readable {
                    return Err(AuthoringError::FieldNotFound);
                }
                if !value.matches(info.kind) {
                    return Err(AuthoringError::WrongType);
                }
                // The inspector keeps the quaternion read-only. The gizmo still sets it.
                if (type_id, field) == (TYPE_SPATIAL_FRAME, FIELD_LOCAL_ROTATION) {
                    let PropertyValue::Quat(rotation) = value else { return Err(AuthoringError::WrongType) };
                    return self.world.set_entity_local_rotation(target, rotation);
                }
                if (type_id, field) == (TYPE_MESH_RENDERER, FIELD_SURFACE) {
                    let PropertyValue::String(choice) = value else { return Err(AuthoringError::WrongType) };
                    return self.world.set_authored_mesh_choice(target, &choice);
                }
                if (type_id, field) == (TYPE_MESH_RENDERER, FIELD_MATERIAL_SLOT) {
                    let PropertyValue::String(name) = value else { return Err(AuthoringError::WrongType) };
                    return self.set_mesh_material_name(target, &name);
                }
                if (type_id, field) == (TYPE_ENTITY, FIELD_PARENT) {
                    let PropertyValue::OptionalEntity(parent) = value else { return Err(AuthoringError::WrongType) };
                    let current = self.world.entity_parent(target)?;
                    if current == parent {
                        return Ok(AuthoringResult::Unchanged);
                    }
                    let handle = self.world.find_entity(target).map_err(|_| AuthoringError::NotFound)?;
                    let parent_handle = match parent {
                        None => None,
                        Some(parent) => Some(self.world.find_entity(parent).map_err(|_| AuthoringError::NotFound)?),
                    };
                    self.world.reparent_entity(handle, parent_handle)?;
                    return Ok(AuthoringResult::Applied);
                }
                if !info.editable {
                    return Err(AuthoringError::ReadOnly);
                }
                match (type_id, field) {
                    (TYPE_ENTITY, FIELD_NAME) => {
                        let PropertyValue::String(name) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_entity_name(target, &name)
                    }
                    (TYPE_SPATIAL_FRAME, FIELD_LOCAL_TRANSLATION) => {
                        let PropertyValue::Vec3(translation) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_entity_local_translation(target, translation)
                    }
                    (TYPE_DIRECTIONAL_LIGHT | TYPE_POINT_LIGHT | TYPE_SPOT_LIGHT, FIELD_COLOR) => {
                        let PropertyValue::Vec3(color) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_light_color(target, color)
                    }
                    (TYPE_DIRECTIONAL_LIGHT | TYPE_POINT_LIGHT | TYPE_SPOT_LIGHT, FIELD_INTENSITY) => {
                        let PropertyValue::F64(intensity) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_light_intensity(target, intensity)
                    }
                    (TYPE_POINT_LIGHT | TYPE_SPOT_LIGHT, FIELD_RANGE) => {
                        let PropertyValue::F64(range_m) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_light_range(target, range_m)
                    }
                    (TYPE_DIRECTIONAL_LIGHT | TYPE_POINT_LIGHT | TYPE_SPOT_LIGHT, FIELD_ENABLED) => {
                        let PropertyValue::Bool(enabled) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_light_enabled(target, enabled)
                    }
                    (
                        TYPE_DIRECTIONAL_LIGHT | TYPE_POINT_LIGHT | TYPE_SPOT_LIGHT,
                        FIELD_CAST_SHADOWS
                        | FIELD_SHADOW_RESOLUTION
                        | FIELD_SHADOW_BIAS
                        | FIELD_SHADOW_SLOPE_BIAS
                        | FIELD_SHADOW_NORMAL_BIAS
                        | FIELD_SHADOW_FILTER
                        | FIELD_SHADOW_DISTANCE
                        | FIELD_CASCADE_COUNT
                        | FIELD_CASCADE_DISTRIBUTION,
                    ) => self.world.set_authored_light_shadow(target, field, value),
                    (TYPE_MESH_RENDERER, FIELD_OBJECT_SCALE) => {
                        let PropertyValue::Vec3(scale) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_object_scale(target, scale)
                    }
                    (TYPE_MESH_RENDERER, FIELD_VISIBLE) => {
                        let PropertyValue::Bool(visible) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_visible(target, visible)
                    }
                    (TYPE_MESH_RENDERER, FIELD_CAST_SHADOWS | FIELD_RECEIVE_SHADOWS) => self.world.set_authored_mesh_shadow(target, field, value),
                    (TYPE_MESH_RENDERER, FIELD_UV_SCALE | FIELD_ROUGHNESS_FACTOR | FIELD_METALLIC_FACTOR | FIELD_NORMAL_SCALE) => {
                        let PropertyValue::F64(number) = value else { return Err(AuthoringError::WrongType) };
                        self.set_mesh_material_scalar(target, field, number)
                    }
                    (TYPE_SPOT_LIGHT, FIELD_INNER_CONE) => {
                        let PropertyValue::F64(radians) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_spot_inner(target, radians)
                    }
                    (TYPE_SPOT_LIGHT, FIELD_OUTER_CONE) => {
                        let PropertyValue::F64(radians) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_spot_outer(target, radians)
                    }
                    (TYPE_REFLECTION_PROBE, FIELD_RADIUS) => {
                        let PropertyValue::F64(radius_m) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_probe_radius(target, radius_m)
                    }
                    (TYPE_REFLECTION_PROBE, FIELD_PRIORITY) => {
                        let PropertyValue::F64(priority) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_probe_priority(target, priority)
                    }
                    (TYPE_REFLECTION_PROBE, FIELD_INTENSITY) => {
                        let PropertyValue::F64(intensity) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_probe_intensity(target, intensity)
                    }
                    (TYPE_REFLECTION_PROBE, FIELD_ENABLED) => {
                        let PropertyValue::Bool(enabled) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_probe_enabled(target, enabled)
                    }
                    (TYPE_REFLECTION_PROBE, FIELD_RESOLUTION) => {
                        let PropertyValue::F64(resolution) = value else { return Err(AuthoringError::WrongType) };
                        if !resolution.is_finite() || resolution < 0.0 || resolution.fract() != 0.0 || resolution > u32::MAX as f64 {
                            return Err(AuthoringError::InvalidValue);
                        }
                        self.world.set_authored_probe_resolution(target, resolution as u32)
                    }
                    (TYPE_REFLECTION_PROBE, FIELD_UPDATE_MODE) => {
                        let PropertyValue::String(label) = value else { return Err(AuthoringError::WrongType) };
                        let policy = jarvig_core::ProbeUpdatePolicy::from_label(&label).ok_or(AuthoringError::InvalidValue)?;
                        Ok(self.world.set_authored_probe_policy(policy))
                    }
                    (TYPE_ENVIRONMENT, FIELD_UPPER_COLOR) => {
                        let PropertyValue::Vec3(color) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_environment_upper(target, color)
                    }
                    (TYPE_ENVIRONMENT, FIELD_LOWER_COLOR) => {
                        let PropertyValue::Vec3(color) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_environment_lower(target, color)
                    }
                    (TYPE_ENVIRONMENT, FIELD_INTENSITY) => {
                        let PropertyValue::F64(intensity) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_environment_intensity(target, intensity)
                    }
                    (TYPE_ENVIRONMENT, FIELD_ENABLED) => {
                        let PropertyValue::Bool(enabled) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_environment_enabled(target, enabled)
                    }
                    (TYPE_CAMERA, FIELD_ENABLED) => {
                        let PropertyValue::Bool(enabled) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_camera_enabled(target, enabled)
                    }
                    (TYPE_CAMERA, FIELD_PROJECTION) => {
                        let PropertyValue::String(projection) = value else { return Err(AuthoringError::WrongType) };
                        let orthographic = match projection.as_str() {
                            "Perspective" => false,
                            "Orthographic" => true,
                            _ => return Err(AuthoringError::InvalidValue),
                        };
                        self.world.set_authored_camera_projection(target, orthographic)
                    }
                    (TYPE_CAMERA, FIELD_VERTICAL_FOV) => {
                        let PropertyValue::F64(degrees) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_camera_fov_deg(target, degrees)
                    }
                    (TYPE_CAMERA, FIELD_ORTHO_HEIGHT) => {
                        let PropertyValue::F64(meters) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_camera_ortho_height(target, meters)
                    }
                    (TYPE_CAMERA, FIELD_NEAR_PLANE) => {
                        let PropertyValue::F64(near_m) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_camera_near(target, near_m)
                    }
                    (TYPE_CAMERA, FIELD_FAR_PLANE) => {
                        let PropertyValue::F64(far_m) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_camera_far(target, far_m)
                    }
                    (TYPE_CAMERA, FIELD_PRIORITY) => {
                        let PropertyValue::F64(priority) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_camera_priority(target, priority)
                    }
                    (TYPE_CAMERA, FIELD_VIEWPORT) => {
                        let PropertyValue::String(viewport) = value else { return Err(AuthoringError::WrongType) };
                        self.world.set_authored_camera_viewport(target, &viewport)
                    }
                    (TYPE_JOINT, _) => {
                        let Some(current) = self.world.authored_joint(target) else { return Err(AuthoringError::InvalidOperation) };
                        let next = jarvig_core::apply_joint_property(&current, field, value)?;
                        self.world.set_authored_joint(target, next)
                    }
                    (TYPE_TERRAIN, _) => {
                        let Some(current) = self.world.authored_terrain(target) else { return Err(AuthoringError::InvalidOperation) };
                        let next = jarvig_core::apply_terrain_property(&current, field, value)?;
                        self.world.set_authored_terrain(target, next)
                    }
                    (TYPE_PLAYER_START, _) => {
                        let Some(current) = self.world.authored_player_start(target) else { return Err(AuthoringError::InvalidOperation) };
                        let next = jarvig_core::apply_player_start_property(&current, field, value)?;
                        self.world.set_authored_player_start(target, next)
                    }
                    (TYPE_PARAMETRIC_BLOCK, _) => self.world.set_block_field(target, field, value),
                    _ => Err(AuthoringError::ReadOnly),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvig_core::{
        AuthoringResult, EntityId, PropertyValue, Vec3, FIELD_LOCAL_ROTATION, FIELD_LOCAL_TRANSLATION, FIELD_NAME, FIELD_PARENT, FIELD_UUID,
        TYPE_ENTITY, TYPE_SPATIAL_FRAME,
    };
    use crate::EngineSession;

    fn editor() -> EngineSession {
        EngineSession::editor().unwrap()
    }

    #[test]
    fn commands_preserve_identity_and_reject_readonly_and_bad_values() {
        let mut engine = editor();
        let near = engine.world().entity_outline()[0].uuid;
        let handle = engine.world().find_entity(near).unwrap();
        let revision = engine.world().revision();
        let selection_note = near;
        let applied = engine
            .execute_authoring(AuthoringCommand::SetProperty {
                target: near,
                type_id: TYPE_ENTITY,
                field: FIELD_NAME,
                value: PropertyValue::String("Player Start".into()),
            })
            .unwrap();
        assert_eq!(applied, AuthoringResult::Applied);
        assert_eq!(engine.world().revision(), revision + 1);
        assert_eq!(engine.world().find_entity(near).unwrap(), handle);
        assert_eq!(engine.world().inspect_entity(near).unwrap().sections[0].fields[0].display, "Player Start");
        let again = engine
            .execute_authoring(AuthoringCommand::SetProperty {
                target: near,
                type_id: TYPE_ENTITY,
                field: FIELD_NAME,
                value: PropertyValue::String("Player Start".into()),
            })
            .unwrap();
        assert_eq!(again, AuthoringResult::Unchanged);
        assert_eq!(engine.world().revision(), revision + 1);
        assert!(matches!(
            engine.execute_authoring(AuthoringCommand::SetProperty {
                target: near,
                type_id: TYPE_ENTITY,
                field: FIELD_UUID,
                value: PropertyValue::Entity(EntityId::new()),
            }),
            Err(jarvig_core::AuthoringError::ReadOnly)
        ));
        assert_eq!(
            engine
                .execute_authoring(AuthoringCommand::SetProperty {
                    target: near,
                    type_id: TYPE_ENTITY,
                    field: FIELD_PARENT,
                    value: PropertyValue::OptionalEntity(None),
                })
                .unwrap(),
            AuthoringResult::Unchanged
        );
        assert_eq!(
            engine
                .execute_authoring(AuthoringCommand::SetProperty {
                    target: near,
                    type_id: TYPE_SPATIAL_FRAME,
                    field: FIELD_LOCAL_ROTATION,
                    value: PropertyValue::Quat(jarvig_core::Quat::IDENTITY),
                })
                .unwrap(),
            AuthoringResult::Unchanged
        );
        assert!(matches!(
            engine.execute_authoring(AuthoringCommand::SetProperty {
                target: near,
                type_id: TYPE_SPATIAL_FRAME,
                field: FIELD_LOCAL_ROTATION,
                value: PropertyValue::Quat(jarvig_core::Quat { x: f64::NAN, y: 0.0, z: 0.0, w: 1.0 }),
            }),
            Err(jarvig_core::AuthoringError::InvalidValue)
        ));
        assert_eq!(engine.world().revision(), revision + 1);
        assert!(matches!(
            engine.execute_authoring(AuthoringCommand::SetProperty {
                target: near,
                type_id: TYPE_ENTITY,
                field: FIELD_NAME,
                value: PropertyValue::F64(1.0),
            }),
            Err(jarvig_core::AuthoringError::WrongType)
        ));
        assert!(matches!(
            engine.execute_authoring(AuthoringCommand::SetProperty {
                target: near,
                type_id: TYPE_ENTITY,
                field: FIELD_LOCAL_TRANSLATION,
                value: PropertyValue::Vec3(Vec3::ZERO),
            }),
            Err(jarvig_core::AuthoringError::FieldNotFound)
        ));
        let compiles = engine.material_compile_count();
        assert!(matches!(
            engine.execute_authoring(AuthoringCommand::AddComponent { target: near, type_id: TYPE_MESH_RENDERER }),
            Err(jarvig_core::AuthoringError::InvalidOperation)
        ));
        assert!(matches!(
            engine.execute_authoring(AuthoringCommand::RemoveComponent { target: near, type_id: TYPE_SPATIAL_FRAME, slot: 0 }),
            Err(jarvig_core::AuthoringError::InvalidOperation)
        ));
        assert_eq!(engine.material_compile_count(), compiles);
        assert!(matches!(
            engine.execute_authoring(AuthoringCommand::SetProperty {
                target: EntityId::new(),
                type_id: TYPE_ENTITY,
                field: FIELD_NAME,
                value: PropertyValue::String("Nope".into()),
            }),
            Err(jarvig_core::AuthoringError::NotFound)
        ));
        let local = match &engine.world().inspect_entity(near).unwrap().sections[1].fields[0].value {
            PropertyValue::Vec3(value) => *value,
            _ => panic!("translation"),
        };
        let moved = Vec3::new(local.x + 0.5, local.y, local.z);
        assert_eq!(
            engine
                .execute_authoring(AuthoringCommand::SetProperty {
                    target: near,
                    type_id: TYPE_SPATIAL_FRAME,
                    field: FIELD_LOCAL_TRANSLATION,
                    value: PropertyValue::Vec3(moved),
                })
                .unwrap(),
            AuthoringResult::Applied
        );
        assert_eq!(
            engine
                .execute_authoring(AuthoringCommand::SetProperty {
                    target: near,
                    type_id: TYPE_SPATIAL_FRAME,
                    field: FIELD_LOCAL_TRANSLATION,
                    value: PropertyValue::Vec3(moved),
                })
                .unwrap(),
            AuthoringResult::Unchanged
        );
        let after_move = engine.world().revision();
        assert!(matches!(
            engine.execute_authoring(AuthoringCommand::SetProperty {
                target: near,
                type_id: TYPE_SPATIAL_FRAME,
                field: FIELD_LOCAL_TRANSLATION,
                value: PropertyValue::Vec3(Vec3::new(f64::INFINITY, 0.0, 0.0)),
            }),
            Err(jarvig_core::AuthoringError::InvalidValue)
        ));
        assert_eq!(engine.world().revision(), after_move);
        assert_eq!(engine.world().find_entity(selection_note).unwrap(), handle);
        assert!(engine.authoring_command_count() >= 2);
        assert!(engine.authoring_failure_count() >= 4);
    }

    #[test]
    fn component_edits_use_the_entity_and_do_not_compile_a_material() {
        let mut engine = editor();
        let compiles = engine.material_compile_count();
        let outline = engine.world().entity_outline();
        let point = outline.iter().find(|row| row.name == "Blue Point Light").unwrap().uuid;
        let directional = outline.iter().find(|row| row.name == "Directional Light").unwrap().uuid;
        let spot = outline.iter().find(|row| row.name == "Warm Spot Light").unwrap().uuid;
        let probe = outline.iter().find(|row| row.name == "Reflection Probe").unwrap().uuid;
        let settings = outline.iter().find(|row| row.name == "World Settings").unwrap().uuid;
        let near = outline[0].uuid;
        let before = engine.world().extract(jarvig_core::RenderFrameId(1)).unwrap();
        let point_id = before.lights().iter().find(|light| light.kind == jarvig_core::LightKind::Point).unwrap().id;
        let probe_id = before.reflection_probes()[0].id;
        let near_material = before.instances()[0].material_for_slot(0);
        assert_eq!(
            engine
                .execute_authoring(AuthoringCommand::SetProperty {
                    target: point,
                    type_id: jarvig_core::TYPE_POINT_LIGHT,
                    field: jarvig_core::FIELD_INTENSITY,
                    value: PropertyValue::F64(14.0),
                })
                .unwrap(),
            AuthoringResult::Unchanged
        );
        assert_eq!(
            engine
                .execute_authoring(AuthoringCommand::SetProperty {
                    target: point,
                    type_id: jarvig_core::TYPE_POINT_LIGHT,
                    field: jarvig_core::FIELD_INTENSITY,
                    value: PropertyValue::F64(16.0),
                })
                .unwrap(),
            AuthoringResult::Applied
        );
        assert_eq!(
            engine
                .execute_authoring(AuthoringCommand::SetProperty {
                    target: probe,
                    type_id: jarvig_core::TYPE_REFLECTION_PROBE,
                    field: jarvig_core::FIELD_RADIUS,
                    value: PropertyValue::F64(8.0),
                })
                .unwrap(),
            AuthoringResult::Unchanged
        );
        assert_eq!(
            engine
                .execute_authoring(AuthoringCommand::SetProperty {
                    target: probe,
                    type_id: jarvig_core::TYPE_REFLECTION_PROBE,
                    field: jarvig_core::FIELD_RADIUS,
                    value: PropertyValue::F64(9.0),
                })
                .unwrap(),
            AuthoringResult::Applied
        );
        assert_eq!(
            engine
                .execute_authoring(AuthoringCommand::SetProperty {
                    target: settings,
                    type_id: jarvig_core::TYPE_ENVIRONMENT,
                    field: jarvig_core::FIELD_INTENSITY,
                    value: PropertyValue::F64(jarvig_core::BOOTSTRAP_ENVIRONMENT_INTENSITY as f64),
                })
                .unwrap(),
            AuthoringResult::Unchanged
        );
        assert_eq!(
            engine
                .execute_authoring(AuthoringCommand::SetProperty {
                    target: spot,
                    type_id: jarvig_core::TYPE_SPOT_LIGHT,
                    field: jarvig_core::FIELD_ENABLED,
                    value: PropertyValue::Bool(false),
                })
                .unwrap(),
            AuthoringResult::Applied
        );
        assert!(matches!(
            engine.execute_authoring(AuthoringCommand::SetProperty {
                target: directional,
                type_id: jarvig_core::TYPE_DIRECTIONAL_LIGHT,
                field: jarvig_core::FIELD_RANGE,
                value: PropertyValue::F64(10.0),
            }),
            Err(jarvig_core::AuthoringError::FieldNotFound)
        ));
        assert!(matches!(
            engine.execute_authoring(AuthoringCommand::SetProperty {
                target: probe,
                type_id: jarvig_core::TYPE_REFLECTION_PROBE,
                field: jarvig_core::FIELD_UPDATE_MODE,
                value: PropertyValue::String("Every Frame".into()),
            }),
            Err(jarvig_core::AuthoringError::InvalidValue)
        ));
        assert!(matches!(
            engine.execute_authoring(AuthoringCommand::SetProperty {
                target: near,
                type_id: jarvig_core::TYPE_MESH_RENDERER,
                field: jarvig_core::FIELD_SURFACE,
                value: PropertyValue::String("Cube".into()),
            }),
            Err(jarvig_core::AuthoringError::ReadOnly)
        ));
        assert!(matches!(
            engine.execute_authoring(AuthoringCommand::SetProperty {
                target: near,
                type_id: jarvig_core::TYPE_POINT_LIGHT,
                field: jarvig_core::FIELD_INTENSITY,
                value: PropertyValue::F64(1.0),
            }),
            Err(jarvig_core::AuthoringError::InvalidOperation)
        ));
        assert_eq!(engine.material_compile_count(), compiles);
        assert_eq!(engine.world().object_count(), 2);
        assert_eq!(engine.world().light_count(), 3);
        let after = engine.world().extract(jarvig_core::RenderFrameId(2)).unwrap();
        assert_eq!(after.instance_count(), 2);
        assert_eq!(after.light_count(), 2);
        assert_eq!(after.reflection_probes().len(), 1);
        assert_eq!(after.reflection_probes()[0].id, probe_id);
        assert!((after.reflection_probes()[0].radius_m - 9.0).abs() < 1.0e-9);
        assert_eq!(after.instances()[0].material_for_slot(0), near_material);
        let point_light = after.lights().iter().find(|light| light.kind == jarvig_core::LightKind::Point).unwrap();
        assert_eq!(point_light.id, point_id);
        assert!((point_light.intensity - 16.0).abs() < 1.0e-5);
        assert!(after.lights().iter().all(|light| light.kind != jarvig_core::LightKind::Spot));
    }

    #[test]
    fn duplicate_and_destroy_do_not_compile_or_give_the_server_a_gpu() {
        let mut server = EngineSession::server().unwrap();
        let outline = server.world().entity_outline();
        let point = outline.iter().find(|row| row.name == "Blue Point Light").unwrap().uuid;
        let settings = outline.iter().find(|row| row.name == "World Settings").unwrap().uuid;
        let tick = server
            .run_frame(1.0 / 60.0, |_snapshot, _meshes, _materials, _textures| -> Result<(), &'static str> {
                panic!("server must not render");
            })
            .unwrap();
        assert!(!tick.render_executed);
        assert_eq!(server.extraction_count(), 0);
        assert_eq!(server.material_compile_count(), 0);
        let created = server.execute_authoring(AuthoringCommand::DuplicateEntity { target: point }).unwrap();
        let AuthoringResult::Duplicated(copy) = created else { panic!("duplicate result") };
        assert_ne!(copy, point);
        assert_eq!(server.world().light_count(), 4);
        assert_eq!(server.material_compile_count(), 0);
        assert_eq!(server.extraction_count(), 0);
        server.execute_authoring(AuthoringCommand::DestroyEntity { target: copy }).unwrap();
        assert_eq!(server.world().light_count(), 3);
        assert!(server.world().entity_ownership(copy).is_err());
        assert!(matches!(
            server.execute_authoring(AuthoringCommand::DuplicateEntity { target: settings }),
            Err(jarvig_core::AuthoringError::ProtectedEntity)
        ));
        assert!(matches!(
            server.execute_authoring(AuthoringCommand::DestroyEntity { target: settings }),
            Err(jarvig_core::AuthoringError::ProtectedEntity)
        ));
        assert!(matches!(
            server.execute_authoring(AuthoringCommand::DestroyEntity { target: EntityId::new() }),
            Err(jarvig_core::AuthoringError::NotFound)
        ));
        assert_eq!(server.material_compile_count(), 0);
        assert_eq!(server.extraction_count(), 0);

        let mut editor = editor();
        let compiles = editor.material_compile_count();
        let probe = editor.world().entity_outline().iter().find(|row| row.name == "Reflection Probe").unwrap().uuid;
        let AuthoringResult::Duplicated(_) = editor.execute_authoring(AuthoringCommand::DuplicateEntity { target: probe }).unwrap() else {
            panic!("probe duplicate")
        };
        assert_eq!(editor.material_compile_count(), compiles);
        assert_eq!(editor.world().reflection_probe_count(), 2);
    }
}

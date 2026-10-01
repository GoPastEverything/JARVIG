//! The host calls [`EngineSession::run_frame`]. The engine decides the order.
//!
//! Clock first. When the profile presents, the engine extracts one
//! [`RenderSceneSnapshot`] from the authoritative scene and hands that to the
//! render callback. The server does not extract. The callback does not see the
//! world. GPU work stays on the thread that owns the device.

use jarvig_core::{
    bootstrap_pbr_textures, Camera, LevelDocument, MaterialAssetRef, MeshAssetRef, MeshLibrary, Profile, RenderFrameId, RenderSceneSnapshot,
    Runtime, RuntimeError, RuntimeTick, SceneWorld, TextureLibrary,
};
use jarvig_material::{MaterialLibrary, ParameterValue, SamplerId, TextureId};

fn object_asset_id(world: &SceneWorld, object: jarvig_core::ObjectId) -> Option<jarvig_core::AssetId> {
    let entity = world.entity(object).ok()?;
    match world.authored_mesh(entity) {
        Some((_, _, _, _, _, _, MeshAssetRef::Asset { id, .. }, _)) => Some(id),
        _ => None,
    }
}

mod authoring;

pub use authoring::AuthoringCommand;

#[derive(Debug)]
pub enum FrameError<E> {
    Runtime(RuntimeError),
    Render(E),
    Scene(jarvig_core::SpaceError),
}

pub struct EngineSession {
    runtime: Runtime,
    world: SceneWorld,
    materials: MaterialLibrary,
    textures: TextureLibrary,
    render_frame: u64,
    extractions: u64,
    extracted_this_frame: bool,
    authoring_commands: u64,
    authoring_failures: u64,
    last_instance_count: u32,
    last_visible_count: u32,
    last_light_count: u32,
    near_color: Option<TextureId>,
    far_color: Option<TextureId>,
    level_sampler: Option<SamplerId>,
    staged: std::collections::HashMap<String, StagedMaterialTextures>,
    imported: std::collections::HashMap<jarvig_core::AssetId, StagedMaterialTextures>,
    mesh_assets: jarvig_core::MeshAssetLibrary,
    /// Standard master from the bootstrap scene. Empty worlds have no mesh to rediscover it.
    standard_master: Option<jarvig_material::MasterMaterialId>,
}

#[derive(Clone, Copy)]
struct StagedMaterialTextures {
    base_color: TextureId,
    orm: TextureId,
    normal: TextureId,
}

impl EngineSession {
    pub fn editor() -> Result<Self, RuntimeError> {
        Self::open(Profile::Editor)
    }

    pub fn server() -> Result<Self, RuntimeError> {
        Self::open(Profile::Server)
    }

    fn open(profile: Profile) -> Result<Self, RuntimeError> {
        let mut world = SceneWorld::bootstrap();
        let (materials, textures, near_color, far_color, level_sampler) = if profile.renders() {
            let (materials, textures, near_color, far_color, sampler) = install_bootstrap_materials(&mut world);
            (materials, textures, Some(near_color), Some(far_color), Some(sampler))
        } else {
            (MaterialLibrary::new(), TextureLibrary::new(), None, None, None)
        };
        let mut session = Self {
            runtime: Runtime::new(profile, 60.0, 8)?,
            world,
            materials,
            textures,
            render_frame: 0,
            extractions: 0,
            extracted_this_frame: false,
            authoring_commands: 0,
            authoring_failures: 0,
            last_instance_count: 0,
            last_visible_count: 0,
            last_light_count: 0,
            near_color,
            far_color,
            level_sampler,
            staged: std::collections::HashMap::new(),
            imported: std::collections::HashMap::new(),
            mesh_assets: jarvig_core::MeshAssetLibrary::default(),
            standard_master: None,
        };
        if profile.renders() {
            if let Ok(master) = session.current_material_master() {
                session.standard_master = Some(master);
            }
        }
        Ok(session)
    }

    pub fn mesh_asset_library(&self) -> &jarvig_core::MeshAssetLibrary {
        &self.mesh_assets
    }

    /// Cluster bounds for one authored entity. Empty for a builtin mesh. Does not copy vertices.
    pub fn meshlet_records(&self, entity: jarvig_core::EntityId) -> Vec<jarvig_core::GpuMeshletRecord> {
        let Some((_, _, _, _, _, _, mesh, _)) = self.world.authored_mesh(entity) else { return Vec::new() };
        let jarvig_core::MeshAssetRef::Asset { id, .. } = mesh else { return Vec::new() };
        self.mesh_assets
            .meshlets(id)
            .map(|set| set.meshlets.iter().map(jarvig_core::GpuMeshletRecord::from_meshlet).collect())
            .unwrap_or_default()
    }

    pub fn mesh_asset_names(&self) -> Vec<String> {
        self.mesh_assets.names()
    }

    pub fn replace_mesh_assets(&mut self, assets: jarvig_core::MeshAssetLibrary) {
        self.cache_imported_textures(&assets);
        self.world.install_imported_meshes(&assets);
        self.mesh_assets = assets;
    }

    fn cache_imported_textures(&mut self, assets: &jarvig_core::MeshAssetLibrary) {
        self.imported.clear();
        if self.textures.white_srgb().is_none() {
            return;
        }
        for record in assets.records() {
            let Some(surface) = assets.imported_surface(record.id) else { continue };
            let base = surface.base_color.clone().map(|texture| self.textures.insert(texture)).or_else(|| self.textures.white_srgb());
            let orm = surface.orm.clone().map(|texture| self.textures.insert(texture)).or_else(|| self.textures.neutral_orm());
            let normal = surface.normal.clone().map(|texture| self.textures.insert(texture)).or_else(|| self.textures.flat_normal());
            if let (Some(base_color), Some(orm), Some(normal)) = (base, orm, normal) {
                self.imported.insert(record.id, StagedMaterialTextures { base_color, orm, normal });
            }
        }
    }

    /// Import one GLB or glTF into the open project and place one actor. The file does not become a level.
    pub fn import_and_place_mesh(&mut self, project_file: &std::path::Path, content_directory: &str, intermediate_directory: &str, source_file: &std::path::Path) -> Result<jarvig_core::EntityId, String> {
        let root = project_file.parent().ok_or("project file has no directory")?;
        let (library, id) = jarvig_core::import_mesh_into_project(root, content_directory, intermediate_directory, source_file)?;
        self.replace_mesh_assets(library);
        let material = self.imported_placement_material(id);
        let name = self.mesh_assets.record(id).map(|record| record.name.clone()).unwrap_or_else(|| "Imported Mesh".into());
        let entity = self.world.place_imported_mesh(id, &name, material.clone()).map_err(|error| error.to_string())?;
        if self.runtime.profile().renders() {
            let master = self.current_material_master()?;
            self.bind_object_materials(entity, &material, master)?;
        }
        Ok(entity)
    }

    /// Place one actor for a mesh that is already in the project library. Does not read the source file.
    pub fn place_existing_mesh(&mut self, id: jarvig_core::AssetId, x: f64, y: f64, z: f64) -> Result<jarvig_core::EntityId, String> {
        if self.mesh_assets.record(id).is_none() {
            return Err("that model is not in the resident library".into());
        }
        let material = self.imported_placement_material(id);
        let name = self.mesh_assets.record(id).map(|record| record.name.clone()).unwrap_or_else(|| "Mesh".into());
        let entity = self.world.place_imported_mesh_at(id, &name, material.clone(), x, y, z).map_err(|error| error.to_string())?;
        if self.runtime.profile().renders() {
            let master = self.current_material_master()?;
            self.bind_object_materials(entity, &material, master)?;
        }
        Ok(entity)
    }

    /// Place a `.jarvigprefab`. New uuids. The file stays on disk.
    pub fn instantiate_prefab_file(&mut self, path: &std::path::Path, origin: jarvig_core::Vec3) -> Result<Vec<jarvig_core::EntityId>, String> {
        let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        let prefab = jarvig_core::parse_prefab(&text).map_err(|error| error.to_string())?;
        prefab.instantiate(self.world_mut(), origin).map_err(|error| error.to_string())
    }

    fn imported_placement_material(&self, id: jarvig_core::AssetId) -> jarvig_core::MaterialAssetRef {
        let surface = self.mesh_assets.imported_surface(id);
        jarvig_core::MaterialAssetRef {
            scheme: jarvig_core::MaterialScheme::Mesh,
            name: "Imported".into(),
            base_color: surface.map(|surface| surface.base_color_factor).unwrap_or([1.0, 1.0, 1.0, 1.0]),
            metallic: surface.map(|surface| surface.metallic).unwrap_or(1.0),
            roughness: surface.map(|surface| surface.roughness).unwrap_or(1.0),
            emissive: [0.0, 0.0, 0.0, 0.0],
            uv_scale: 1.0,
            normal_scale: surface.map(|surface| surface.normal_scale).unwrap_or(1.0),
            normal_convention: Some(jarvig_core::NormalConvention::DirectXNegativeY),
        }
    }

    fn bind_object_materials(&mut self, entity: jarvig_core::EntityId, material: &jarvig_core::MaterialAssetRef, master: jarvig_material::MasterMaterialId) -> Result<(), String> {
        let object = self.world.objects().find(|object| self.world.entity(*object).ok() == Some(entity)).ok_or("placed mesh is missing")?;
        self.bind_object(object, material, master)
    }

    /// Register decoded maps for a staged set. Call this before [`Self::load_level`].
    /// The name is the set id, not a path. Replacing a name drops the previous logical textures from use.
    pub fn register_staged_material(&mut self, name: &str, base_color: jarvig_core::Texture, orm: jarvig_core::Texture, normal: jarvig_core::Texture) {
        let base_color = self.textures.insert(base_color);
        let orm = self.textures.insert(orm);
        let normal = self.textures.insert(normal);
        self.staged.insert(name.to_string(), StagedMaterialTextures { base_color, orm, normal });
    }

    pub fn staged_material_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.staged.keys().cloned().collect();
        names.sort();
        names
    }

    /// Points one mesh at a loaded staged set. Factors stay on this actor. The master is not recompiled.
    pub fn set_mesh_material_name(&mut self, target: jarvig_core::EntityId, name: &str) -> Result<jarvig_core::AuthoringResult, jarvig_core::AuthoringError> {
        if name == "Imported" {
            return Ok(jarvig_core::AuthoringResult::Unchanged);
        }
        if !self.staged.contains_key(name) {
            return Err(jarvig_core::AuthoringError::InvalidValue);
        }
        let Some((_, _, _, _, _, _, mesh, mut material)) = self.world.authored_mesh(target) else {
            return Err(jarvig_core::AuthoringError::InvalidOperation);
        };
        if material.scheme == jarvig_core::MaterialScheme::Staged && material.name == name {
            return Ok(jarvig_core::AuthoringResult::Unchanged);
        }
        material.scheme = jarvig_core::MaterialScheme::Staged;
        material.name = name.to_string();
        let object = self.world.entity_ownership(target).map_err(|_| jarvig_core::AuthoringError::NotFound)?.object.ok_or(jarvig_core::AuthoringError::InvalidOperation)?;
        let master = self.current_material_master().map_err(|_| jarvig_core::AuthoringError::InvalidOperation)?;
        let staged = self.staged.get(name).ok_or(jarvig_core::AuthoringError::InvalidValue)?.clone();
        let white = self.textures.white_srgb().ok_or(jarvig_core::AuthoringError::InvalidOperation)?;
        let sampler = self.level_sampler.ok_or(jarvig_core::AuthoringError::InvalidOperation)?;
        let instance = self
            .materials
            .create_instance(
                master,
                &[
                    ("BaseColor", ParameterValue::Texture(staged.base_color)),
                    ("Orm", ParameterValue::Texture(staged.orm)),
                    ("Normal", ParameterValue::Texture(staged.normal)),
                    ("Emissive", ParameterValue::Texture(white)),
                    ("MaterialSampler", ParameterValue::Sampler(sampler)),
                    ("BaseColorFactor", ParameterValue::Float4(material.base_color)),
                    ("MetallicFactor", ParameterValue::Float(material.metallic)),
                    ("RoughnessFactor", ParameterValue::Float(material.roughness)),
                    ("EmissiveFactor", ParameterValue::Float4(material.emissive)),
                    ("NormalScale", ParameterValue::Float(material.normal_scale)),
                    ("UvScale", ParameterValue::Float(material.uv_scale)),
                    ("OcclusionStrength", ParameterValue::Float(1.0)),
                ],
            )
            .map_err(|_| jarvig_core::AuthoringError::InvalidOperation)?;
        let slots = self.world.mesh_material_slots(object);
        let slots = if slots.is_empty() { vec![0] } else { slots };
        for slot in slots {
            self.world.bind_material(object, slot, instance).map_err(|_| jarvig_core::AuthoringError::InvalidOperation)?;
        }
        self.world.set_authored_assets(object, mesh, material).map_err(|_| jarvig_core::AuthoringError::InvalidOperation)?;
        Ok(jarvig_core::AuthoringResult::Applied)
    }

    /// Replace the authoritative world from a validated level document.
    /// The server binds no materials and does not extract. GPU resources are not read or written.
    pub fn load_level(&mut self, document: &LevelDocument) -> Result<(), String> {
        document.validate().map_err(|error| error.to_string())?;
        let master = if self.runtime.profile().renders() { Some(self.current_material_master()?) } else { None };
        let mut world = document.instantiate_with(&self.mesh_assets).map_err(|error| error.to_string())?;
        if let Some(master) = master {
            self.bind_level_materials(&mut world, master)?;
        }
        self.world = world;
        Ok(())
    }

    /// Authored document for the live world. Runtime handles are not included.
    pub fn export_level(&mut self, level_uuid: jarvig_core::EntityId, name: &str) -> Result<LevelDocument, String> {
        self.sync_saved_material_factors();
        LevelDocument::capture(self.world(), level_uuid, name).map_err(|error| error.to_string())
    }

    fn current_material_master(&self) -> Result<jarvig_material::MasterMaterialId, String> {
        if let Some(object) = self.world.objects().next() {
            if let Some(instance) = self.world.material_instance(object, 0) {
                if let Ok(master) = self.materials.master_of(instance) {
                    return Ok(master);
                }
            }
        }
        self.standard_master.clone().ok_or_else(|| "no mesh is available to find the standard material".to_string())
    }

    /// Binds chunk meshes that have an authored material and no instance yet.
    fn bind_new_chunk_materials(&mut self, master: jarvig_material::MasterMaterialId) -> Result<(), String> {
        let objects = self.world.terrain_chunk_objects();
        for object in objects {
            if self.world.material_instance(object, 0).is_some() {
                continue;
            }
            let entity = self.world.entity(object).map_err(|error| error.to_string())?;
            let Some((_, _, _, _, _, _, _, material)) = self.world.authored_mesh(entity) else { continue };
            self.bind_object(object, &material, master)?;
        }
        Ok(())
    }

    fn bind_level_materials(&mut self, world: &mut SceneWorld, master: jarvig_material::MasterMaterialId) -> Result<(), String> {
        let white = self.textures.white_srgb().ok_or("white texture missing")?;
        let orm = self.textures.neutral_orm().ok_or("orm texture missing")?;
        let normal = self.textures.flat_normal().ok_or("normal texture missing")?;
        let sampler = self.level_sampler.ok_or("material sampler missing")?;
        let near_color = self.near_color.ok_or("near texture missing")?;
        let far_color = self.far_color.ok_or("far texture missing")?;
        let objects: Vec<_> = world.objects().collect();
        for object in objects {
            let entity = world.entity(object).map_err(|error| error.to_string())?;
            let Some((_, _, _, _, _, _, _, material)) = world.authored_mesh(entity) else { continue };
            let asset = object_asset_id(world, object);
            let (base, orm_tex, normal_tex) = self.textures_for(asset, &material, white, orm, normal, near_color, far_color)?;
            self.bind_resolved(world, object, master, base, orm_tex, normal_tex, white, sampler, &material)?;
        }
        Ok(())
    }

    fn bind_object(&mut self, object: jarvig_core::ObjectId, material: &jarvig_core::MaterialAssetRef, master: jarvig_material::MasterMaterialId) -> Result<(), String> {
        let white = self.textures.white_srgb().ok_or("white texture missing")?;
        let orm = self.textures.neutral_orm().ok_or("orm texture missing")?;
        let normal = self.textures.flat_normal().ok_or("normal texture missing")?;
        let sampler = self.level_sampler.ok_or("material sampler missing")?;
        let asset = object_asset_id(&self.world, object);
        let (base, orm_tex, normal_tex) = self.textures_for(asset, material, white, orm, normal, white, white)?;
        let instance = self
            .materials
            .create_instance(
                master,
                &[
                    ("BaseColor", ParameterValue::Texture(base)),
                    ("Orm", ParameterValue::Texture(orm_tex)),
                    ("Normal", ParameterValue::Texture(normal_tex)),
                    ("Emissive", ParameterValue::Texture(white)),
                    ("MaterialSampler", ParameterValue::Sampler(sampler)),
                    ("BaseColorFactor", ParameterValue::Float4(material.base_color)),
                    ("MetallicFactor", ParameterValue::Float(material.metallic)),
                    ("RoughnessFactor", ParameterValue::Float(material.roughness)),
                    ("EmissiveFactor", ParameterValue::Float4(material.emissive)),
                    ("NormalScale", ParameterValue::Float(material.normal_scale)),
                    ("UvScale", ParameterValue::Float(material.uv_scale)),
                    ("OcclusionStrength", ParameterValue::Float(1.0)),
                ],
            )
            .map_err(|error| error.to_string())?;
        let slots = self.world.mesh_material_slots(object);
        let slots = if slots.is_empty() { vec![0] } else { slots };
        for slot in slots {
            self.world.bind_material(object, slot, instance).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn textures_for(
        &self,
        asset: Option<jarvig_core::AssetId>,
        material: &jarvig_core::MaterialAssetRef,
        white: jarvig_material::TextureId,
        orm: jarvig_material::TextureId,
        normal: jarvig_material::TextureId,
        near_color: jarvig_material::TextureId,
        far_color: jarvig_material::TextureId,
    ) -> Result<(jarvig_material::TextureId, jarvig_material::TextureId, jarvig_material::TextureId), String> {
        if material.scheme == jarvig_core::MaterialScheme::Mesh {
            if let Some(asset) = asset.and_then(|asset| self.imported.get(&asset)) {
                return Ok((asset.base_color, asset.orm, asset.normal));
            }
            return Ok((white, orm, normal));
        }
        if material.scheme == jarvig_core::MaterialScheme::Staged {
            let staged = self.staged.get(&material.name).ok_or_else(|| format!("staged material {} is not loaded", material.name))?;
            return Ok((staged.base_color, staged.orm, staged.normal));
        }
        let base = match material.name.as_str() {
            "bootstrap_near" => near_color,
            "bootstrap_far" => far_color,
            "standard_white" => white,
            other => return Err(format!("unknown builtin material {other}")),
        };
        Ok((base, orm, normal))
    }

    fn bind_resolved(
        &mut self,
        world: &mut jarvig_core::SceneWorld,
        object: jarvig_core::ObjectId,
        master: jarvig_material::MasterMaterialId,
        base: jarvig_material::TextureId,
        orm_tex: jarvig_material::TextureId,
        normal_tex: jarvig_material::TextureId,
        white: jarvig_material::TextureId,
        sampler: jarvig_material::SamplerId,
        material: &jarvig_core::MaterialAssetRef,
    ) -> Result<(), String> {
        let instance = self
            .materials
            .create_instance(
                master,
                &[
                    ("BaseColor", ParameterValue::Texture(base)),
                    ("Orm", ParameterValue::Texture(orm_tex)),
                    ("Normal", ParameterValue::Texture(normal_tex)),
                    ("Emissive", ParameterValue::Texture(white)),
                    ("MaterialSampler", ParameterValue::Sampler(sampler)),
                    ("BaseColorFactor", ParameterValue::Float4(material.base_color)),
                    ("MetallicFactor", ParameterValue::Float(material.metallic)),
                    ("RoughnessFactor", ParameterValue::Float(material.roughness)),
                    ("EmissiveFactor", ParameterValue::Float4(material.emissive)),
                    ("NormalScale", ParameterValue::Float(material.normal_scale)),
                    ("UvScale", ParameterValue::Float(material.uv_scale)),
                    ("OcclusionStrength", ParameterValue::Float(1.0)),
                ],
            )
            .map_err(|error| error.to_string())?;
        let slots = world.mesh_material_slots(object);
        let slots = if slots.is_empty() { vec![0] } else { slots };
        for slot in slots {
            world.bind_material(object, slot, instance).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn sync_saved_material_factors(&mut self) {
        let objects: Vec<_> = self.world.objects().collect();
        for object in objects {
            let Ok(entity) = self.world.entity(object) else { continue };
            let Some((_, _, _, _, _, _, mesh, mut material)) = self.world.authored_mesh(entity) else { continue };
            let Some(instance) = self.world.material_instance(object, 0) else { continue };
            if let Ok(value) = self.materials.uniform_parameter(instance, "BaseColorFactor") {
                material.base_color = value;
            }
            if let Ok(value) = self.materials.uniform_parameter(instance, "MetallicFactor") {
                material.metallic = value[0];
            }
            if let Ok(value) = self.materials.uniform_parameter(instance, "RoughnessFactor") {
                material.roughness = value[0];
            }
            if let Ok(value) = self.materials.uniform_parameter(instance, "EmissiveFactor") {
                material.emissive = value;
            }
            if let Ok(value) = self.materials.uniform_parameter(instance, "UvScale") {
                material.uv_scale = value[0];
            }
            if let Ok(value) = self.materials.uniform_parameter(instance, "NormalScale") {
                material.normal_scale = value[0];
            }
            let _ = self.world.set_authored_assets(object, mesh, material);
        }
    }

    pub fn front_camera(&self) -> Camera {
        self.world.front_camera()
    }

    pub fn side_camera(&self) -> Camera {
        self.world.side_camera()
    }

    pub fn world(&self) -> &SceneWorld {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut SceneWorld {
        &mut self.world
    }

    pub fn mesh_count(&self) -> usize {
        self.world.mesh_count()
    }

    pub fn extraction_count(&self) -> u64 {
        self.extractions
    }

    pub fn extracted_this_frame(&self) -> bool {
        self.extracted_this_frame
    }

    pub fn render_frame(&self) -> u64 {
        self.render_frame
    }

    pub fn last_instance_count(&self) -> u32 {
        self.last_instance_count
    }

    pub fn last_visible_count(&self) -> u32 {
        self.last_visible_count
    }

    pub fn materials(&self) -> &MaterialLibrary {
        &self.materials
    }

    pub fn material_master_count(&self) -> usize {
        self.materials.master_count()
    }

    pub fn material_instance_count(&self) -> usize {
        self.materials.instance_count()
    }

    pub fn material_compile_count(&self) -> u32 {
        self.materials.compile_count()
    }

    pub fn textures(&self) -> &TextureLibrary {
        &self.textures
    }

    pub fn texture_count(&self) -> usize {
        self.textures.texture_count()
    }

    pub fn sampler_count(&self) -> usize {
        self.textures.sampler_count()
    }

    pub fn pbr_master_count(&self) -> usize {
        self.materials.pbr_master_count()
    }

    pub fn pbr_instance_count(&self) -> usize {
        self.materials.pbr_instance_count()
    }

    pub fn pbr_compile_count(&self) -> u32 {
        self.materials.pbr_compile_count()
    }

    pub fn world_light_count(&self) -> usize {
        self.world.light_count()
    }

    pub fn directional_light_count(&self) -> usize {
        self.world.light_count_of(jarvig_core::LightKind::Directional)
    }

    pub fn point_light_count(&self) -> usize {
        self.world.light_count_of(jarvig_core::LightKind::Point)
    }

    pub fn spot_light_count(&self) -> usize {
        self.world.light_count_of(jarvig_core::LightKind::Spot)
    }

    pub fn last_light_count(&self) -> u32 {
        self.last_light_count
    }

    /// UV scale, roughness multiplier, metallic multiplier, or normal strength.
    /// Updates the authored material and the instance uniform. Does not compile a shader.
    pub fn set_mesh_material_scalar(&mut self, target: jarvig_core::EntityId, field: jarvig_core::FieldId, value: f64) -> Result<jarvig_core::AuthoringResult, jarvig_core::AuthoringError> {
        let Some(object) = self.world.objects().find(|id| self.world.entity(*id).ok() == Some(target)) else {
            return Err(jarvig_core::AuthoringError::NotFound);
        };
        let Some(instance) = self.world.material_instance(object, 0) else { return Err(jarvig_core::AuthoringError::NotFound) };
        let Some((_, _, _, _, _, _, mesh, mut material)) = self.world.authored_mesh(target) else { return Err(jarvig_core::AuthoringError::NotFound) };
        let number = value as f32;
        if !number.is_finite() {
            return Err(jarvig_core::AuthoringError::InvalidOperation);
        }
        let uniform = if field == jarvig_core::FIELD_UV_SCALE {
            if number <= 0.0 { return Err(jarvig_core::AuthoringError::InvalidOperation); }
            material.uv_scale = number;
            "UvScale"
        } else if field == jarvig_core::FIELD_ROUGHNESS_FACTOR {
            material.roughness = number.clamp(0.0, 2.0);
            "RoughnessFactor"
        } else if field == jarvig_core::FIELD_METALLIC_FACTOR {
            material.metallic = number.clamp(0.0, 1.0);
            "MetallicFactor"
        } else if field == jarvig_core::FIELD_NORMAL_SCALE {
            if number < 0.0 { return Err(jarvig_core::AuthoringError::InvalidOperation); }
            material.normal_scale = number;
            "NormalScale"
        } else {
            return Err(jarvig_core::AuthoringError::InvalidOperation);
        };
        let stored = if field == jarvig_core::FIELD_ROUGHNESS_FACTOR {
            number.clamp(0.0, 2.0)
        } else if field == jarvig_core::FIELD_METALLIC_FACTOR {
            number.clamp(0.0, 1.0)
        } else {
            number
        };
        self.materials.set_parameter(instance, uniform, ParameterValue::Float(stored)).map_err(|_| jarvig_core::AuthoringError::InvalidOperation)?;
        let _ = mesh;
        self.world.replace_authored_material(object, material).map_err(|_| jarvig_core::AuthoringError::InvalidOperation)?;
        Ok(jarvig_core::AuthoringResult::Applied)
    }

    /// Runtime factor on one instance. Does not compile a master.
    pub fn set_material_float(
        &mut self,
        instance: jarvig_core::MaterialInstanceId,
        name: &str,
        value: f32,
    ) -> Result<(), jarvig_material::MaterialError> {
        self.materials.set_parameter(instance, name, jarvig_material::ParameterValue::Float(value))
    }

    /// Simulation tick, then at most one extraction, then the render callback.
    /// The callback receives the snapshot and the logical material library, not the world.
    pub fn run_frame<E>(
        &mut self,
        delta_seconds: f64,
        render: impl FnOnce(&RenderSceneSnapshot, &MeshLibrary, &MaterialLibrary, &TextureLibrary) -> Result<(), E>,
    ) -> Result<RuntimeTick, FrameError<E>> {
        let tick = self.runtime.tick(delta_seconds).map_err(FrameError::Runtime)?;
        self.extracted_this_frame = false;
        if tick.render_executed {
            self.world.set_simulation_tick(tick.frame);
            self.render_frame = self.render_frame.saturating_add(1);
            let snapshot = self.world.extract(RenderFrameId(self.render_frame)).map_err(FrameError::Scene)?;
            self.extractions = self.extractions.saturating_add(1);
            self.extracted_this_frame = true;
            self.last_instance_count = snapshot.instance_count() as u32;
            self.last_visible_count = snapshot.visible_count() as u32;
            self.last_light_count = snapshot.light_count() as u32;
            render(&snapshot, self.world.meshes(), &self.materials, &self.textures).map_err(FrameError::Render)?;
        }
        Ok(tick)
    }
}

impl EngineSession {
    /// Validation geometry for lighting. Not the asset database and not imported.
    /// The two bootstrap cards stay. This does not compile a new master.
    pub fn install_lighting_lab(&mut self) -> Result<(), String> {
        let snapshot = self.world.extract(RenderFrameId(0)).map_err(|error| error.to_string())?;
        let near = snapshot.instances().first().and_then(|instance| instance.material_for_slot(0)).ok_or("near material missing")?;
        let master = self.materials.master_of(near).map_err(|error| error.to_string())?;
        let white = self.textures.white_srgb().ok_or("white texture missing")?;
        let orm = self.textures.neutral_orm().ok_or("orm texture missing")?;
        let normal = self.textures.flat_normal().ok_or("normal texture missing")?;
        let sampler = self.textures.insert_sampler(jarvig_material::SamplerState::linear_repeat());
        let scene = self.world.scene_frame().map_err(|error| error.to_string())?;
        let specs = [
            ("Floor", jarvig_core::floor_mesh(6.0, 6.0), jarvig_core::Vec3::new(0.0, -1.15, -4.0), [0.62, 0.62, 0.60, 1.0], 0.0, 0.92, [0.0, 0.0, 0.0, 0.0]),
            ("White Cube", jarvig_core::cube_mesh(1.0), jarvig_core::Vec3::new(-1.55, -0.65, -4.2), [0.82, 0.82, 0.80, 1.0], 0.0, 0.78, [0.0, 0.0, 0.0, 0.0]),
            ("Metal Sphere", jarvig_core::sphere_mesh(0.5, 32, 24), jarvig_core::Vec3::new(1.25, -0.65, -3.7), [0.92, 0.92, 0.94, 1.0], 1.0, 0.045, [0.0, 0.0, 0.0, 0.0]),
            ("Flat Sphere", jarvig_core::flat_sphere_mesh(0.32, 32, 24), jarvig_core::Vec3::new(0.35, -0.83, -5.15), [0.92, 0.92, 0.94, 1.0], 1.0, 0.045, [0.0, 0.0, 0.0, 0.0]),
            (
                "Emissive Panel",
                jarvig_core::emissive_panel_mesh(1.4, 0.9),
                jarvig_core::Vec3::new(2.35, -0.2, -4.0),
                [0.02, 0.02, 0.02, 1.0],
                0.0,
                0.5,
                [8.0, 0.15, 0.08, 1.0],
            ),
        ];
        for (name, mesh, local, color, metal, rough, emissive) in specs {
            let mesh_id = self.world.add_mesh(mesh);
            let object = self.world.spawn_named_object(name, mesh_id, scene, local, jarvig_core::Vec3::new(1.0, 1.0, 1.0)).map_err(|error| error.to_string())?;
            let instance = self
                .materials
                .create_instance(
                    master,
                    &[
                        ("BaseColor", jarvig_material::ParameterValue::Texture(white)),
                        ("Orm", jarvig_material::ParameterValue::Texture(orm)),
                        ("Normal", jarvig_material::ParameterValue::Texture(normal)),
                        ("Emissive", jarvig_material::ParameterValue::Texture(white)),
                        ("MaterialSampler", jarvig_material::ParameterValue::Sampler(sampler)),
                        ("BaseColorFactor", jarvig_material::ParameterValue::Float4(color)),
                        ("MetallicFactor", jarvig_material::ParameterValue::Float(metal)),
                        ("RoughnessFactor", jarvig_material::ParameterValue::Float(rough)),
                        ("EmissiveFactor", jarvig_material::ParameterValue::Float4(emissive)),
                        ("NormalScale", jarvig_material::ParameterValue::Float(1.0)),
                        ("OcclusionStrength", jarvig_material::ParameterValue::Float(1.0)),
                    ],
                )
                .map_err(|error| error.to_string())?;
            self.world.bind_material(object, 0, instance).map_err(|error| error.to_string())?;
            let mesh_ref = match name {
                "Floor" => MeshAssetRef::Floor { width_m: 6.0, depth_m: 6.0 },
                "White Cube" => MeshAssetRef::Cube { size_m: 1.0 },
                "Metal Sphere" => MeshAssetRef::Sphere { radius_m: 0.5, segments: 32, rings: 24, flat: false },
                "Flat Sphere" => MeshAssetRef::Sphere { radius_m: 0.32, segments: 32, rings: 24, flat: true },
                "Emissive Panel" => MeshAssetRef::EmissivePanel { width_m: 1.4, height_m: 0.9 },
                other => return Err(format!("lab mesh {other} has no asset reference")),
            };
            self.world
                .set_authored_assets(
                    object,
                    mesh_ref,
                    MaterialAssetRef::builtin("standard_white", color, metal, rough, emissive),
                )
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

/// Editor and client only. The server keeps an empty library and does not compile.
fn install_bootstrap_materials(world: &mut SceneWorld) -> (MaterialLibrary, TextureLibrary, TextureId, TextureId, SamplerId) {
    let (textures, near_color, far_color, sampler) = bootstrap_pbr_textures();
    let orm = textures.neutral_orm().expect("orm default");
    let normal = textures.flat_normal().expect("normal default");
    // White, so EmissiveFactor is the contribution. Unbound emissive stays black.
    let emissive = textures.white_srgb().expect("emissive identity");
    let (materials, near, far) = MaterialLibrary::bootstrap_standard(near_color, far_color, orm, normal, emissive, sampler);
    let objects: Vec<_> = world.objects().collect();
    world.bind_material(objects[0], 0, near).expect("near material slot");
    world.bind_material(objects[1], 0, far).expect("far material slot");
    let _ = world.set_authored_assets(
        objects[0],
        MeshAssetRef::NearTriangle,
        MaterialAssetRef::builtin("bootstrap_near", [1.0, 1.0, 1.0, 1.0], 0.0, 0.85, [0.0, 0.0, 0.0, 0.0]),
    );
    let _ = world.set_authored_assets(
        objects[1],
        MeshAssetRef::FarTriangle,
        MaterialAssetRef::builtin("bootstrap_far", [1.0, 1.0, 1.0, 1.0], 1.0, 0.2, [0.35, 0.12, 0.02, 0.0]),
    );
    (materials, textures, near_color, far_color, sampler)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_loads_the_lighting_lab_without_a_gpu_and_the_editor_round_trips_a_move() {
        let document = jarvig_core::lighting_lab_level();
        let mut server = EngineSession::server().unwrap();
        server.load_level(&document).unwrap();
        assert_eq!(server.world().entity_count(), 12);
        assert_eq!(server.mesh_count(), 7);
        assert_eq!(server.material_compile_count(), 0);
        server.run_frame(1.0 / 60.0, |_, _, _, _| -> Result<(), ()> { panic!("server rendered") }).unwrap();
        let mut editor = EngineSession::editor().unwrap();
        let compiles = editor.material_compile_count();
        editor.load_level(&document).unwrap();
        assert_eq!(editor.material_compile_count(), compiles);
        assert_eq!(editor.world().object_count(), 7);
        let cube = jarvig_core::EntityId::parse("99999999-9999-4999-8999-999999999999").unwrap();
        editor.world_mut().set_entity_local_translation(cube, jarvig_core::Vec3::new(-1.0, -0.65, -4.2)).unwrap();
        let exported = editor.export_level(document.level_uuid, "Lighting Lab").unwrap();
        let text = exported.to_json();
        assert!(!text.contains("MaterialInstanceId"));
        let mut restored = EngineSession::editor().unwrap();
        restored.load_level(&jarvig_core::parse_level(&text).unwrap()).unwrap();
        let (translation, _, _, _, _, _, _, material) = restored.world().authored_mesh(cube).unwrap();
        assert!((translation.x + 1.0).abs() < 1.0e-6);
        assert!((material.roughness - 0.78).abs() < 1.0e-5);
        assert_eq!(restored.world().entity_count(), 12);
        assert_eq!(restored.world().environment().intensity, 0.20);
    }

    #[test]
    fn material_instance_factors_round_trip_without_a_shader_or_pixels() {
        let document = jarvig_core::lighting_lab_level();
        let mut editor = EngineSession::editor().unwrap();
        let compiles = editor.material_compile_count();
        editor.load_level(&document).unwrap();
        let floor = jarvig_core::EntityId::parse("88888888-8888-4888-8888-888888888888").unwrap();
        editor.set_mesh_material_scalar(floor, jarvig_core::FIELD_UV_SCALE, 0.5).unwrap();
        editor.set_mesh_material_scalar(floor, jarvig_core::FIELD_ROUGHNESS_FACTOR, 2.0).unwrap();
        editor.set_mesh_material_scalar(floor, jarvig_core::FIELD_METALLIC_FACTOR, 0.25).unwrap();
        editor.set_mesh_material_scalar(floor, jarvig_core::FIELD_NORMAL_SCALE, 0.0).unwrap();
        assert_eq!(editor.material_compile_count(), compiles);
        editor.set_mesh_material_scalar(floor, jarvig_core::FIELD_ROUGHNESS_FACTOR, 4.0).unwrap();
        let (_, _, _, _, _, _, _, material) = editor.world().authored_mesh(floor).unwrap();
        assert!((material.uv_scale - 0.5).abs() < 1.0e-6);
        assert!((material.roughness - 2.0).abs() < 1.0e-6);
        assert!((material.metallic - 0.25).abs() < 1.0e-6);
        assert!(material.normal_scale.abs() < 1.0e-6);
        let exported = editor.export_level(document.level_uuid, "Lighting Lab").unwrap();
        let text = exported.to_json();
        assert!(!text.contains("PNG") && !text.contains("pixels"));
        let parsed = jarvig_core::parse_level(&text).unwrap();
        let floor_entity = parsed.entities.iter().find(|entity| entity.name == "Floor").unwrap();
        let saved = floor_entity.components.iter().find_map(|component| match component {
            jarvig_core::ComponentRecord::MeshRenderer { material, .. } => Some(material),
            _ => None,
        }).unwrap();
        assert!((saved.uv_scale - 0.5).abs() < 1.0e-6);
        assert!((saved.roughness - 2.0).abs() < 1.0e-6);
        assert!((saved.metallic - 0.25).abs() < 1.0e-6);
        assert!(saved.normal_scale.abs() < 1.0e-6);
        assert_eq!(editor.material_compile_count(), compiles);
    }

    #[test]
    fn server_does_not_extract_or_call_the_renderer() {
        let mut engine = EngineSession::server().unwrap();
        let tick = engine
            .run_frame(1.0 / 60.0, |_snapshot, _meshes, _materials, _textures| -> Result<(), &'static str> {
                panic!("server must not render");
            })
            .unwrap();
        assert!(!tick.render_executed);
        assert_eq!(engine.extraction_count(), 0);
        assert!(!engine.extracted_this_frame());
        assert_eq!(engine.render_frame(), 0);
        assert_eq!(engine.mesh_count(), 2);
        assert_eq!(engine.material_compile_count(), 0);
        assert_eq!(engine.material_master_count(), 0);
        assert_eq!(engine.material_instance_count(), 0);
        assert_eq!(engine.texture_count(), 0);
        assert_eq!(engine.sampler_count(), 0);
        assert_eq!(engine.world_light_count(), 3);
        assert_eq!(engine.last_light_count(), 0);
        for row in engine.world().entity_outline() {
            assert!(engine.world().component_stack(row.uuid).unwrap().iter().all(|item| item.role != jarvig_core::ComponentRole::Camera));
        }
        assert_eq!(engine.extraction_count(), 0);
        let marker = engine.world_mut().create_entity("Server Camera");
        let id = engine.world().resolve(marker).unwrap();
        engine.world_mut().add_component(id, jarvig_core::TYPE_SPATIAL_FRAME).unwrap();
        engine.world_mut().add_component(id, jarvig_core::TYPE_CAMERA).unwrap();
        assert_eq!(engine.extraction_count(), 0);
        assert!(engine.world().authored_camera(id).is_some());
    }

    #[test]
    fn server_runs_a_game_application_without_a_renderer() {
        let mut engine = EngineSession::server().unwrap();
        let document = jarvig_core::lighting_lab_level();
        engine.load_level(&document).unwrap();
        let revision = engine.world().revision();
        let floor = jarvig_core::EntityId::parse("88888888-8888-4888-8888-888888888888").unwrap();
        let pose = engine.world().entity_world_pose(floor).unwrap();
        let mut app = jarvig_core::GameApplication::new();
        app.load(document).unwrap();
        app.start().unwrap();
        let spawned = app.runtime_world().unwrap().entity_world_pose(floor).unwrap();
        app.tick(1.0 / 60.0).unwrap();
        app.runtime_world_mut().unwrap().set_entity_local_translation(floor, jarvig_core::Vec3::new(9.0, 9.0, 9.0)).unwrap();
        assert_ne!(app.runtime_world().unwrap().entity_world_pose(floor).unwrap().translation, spawned.translation);
        app.stop().unwrap();
        app.start().unwrap();
        assert_eq!(app.runtime_world().unwrap().entity_world_pose(floor).unwrap().translation, spawned.translation);
        app.stop().unwrap();
        assert_eq!(engine.extraction_count(), 0);
        assert_eq!(engine.material_compile_count(), 0);
        assert_eq!(engine.material_instance_count(), 0);
        assert_eq!(engine.texture_count(), 0);
        assert_eq!(engine.world().revision(), revision);
        assert_eq!(engine.world().entity_world_pose(floor).unwrap().translation, pose.translation);
    }

    #[test]
    fn editor_extracts_once_and_does_not_hand_the_world_to_the_callback() {
        let mut engine = EngineSession::editor().unwrap();
        let mut calls = 0;
        let tick = engine
            .run_frame(1.0 / 60.0, |snapshot, meshes, materials, textures| {
                calls += 1;
                assert_eq!(snapshot.render_frame, RenderFrameId(1));
                assert_eq!(snapshot.simulation_tick, 1);
                assert_eq!(snapshot.instance_count(), 2);
                assert_eq!(snapshot.visible_count(), 2);
                assert_eq!(meshes.len(), 2);
                assert!(meshes.get(snapshot.instances()[0].mesh).is_some());
                assert!(snapshot.instances()[0].material_for_slot(0).is_some());
                assert!(snapshot.instances()[1].material_for_slot(0).is_some());
                assert_ne!(
                    snapshot.instances()[0].material_for_slot(0),
                    snapshot.instances()[1].material_for_slot(0)
                );
                assert_eq!(materials.compile_count(), 1);
                assert_eq!(materials.master_count(), 1);
                assert_eq!(materials.instance_count(), 2);
                assert_eq!(textures.texture_count(), 9);
                assert_eq!(textures.sampler_count(), 1);
                assert_eq!(materials.pbr_master_count(), 1);
                assert_eq!(materials.pbr_instance_count(), 2);
                assert_eq!(materials.pbr_compile_count(), 1);
                assert_eq!(snapshot.light_count(), 3);
                assert_eq!(snapshot.lights().len(), snapshot.light_count());
                assert!(snapshot.environment().enabled);
                assert_eq!(snapshot.environment().intensity, jarvig_core::BOOTSTRAP_ENVIRONMENT_INTENSITY);
                assert_eq!(textures.get(textures.neutral_orm().unwrap()).unwrap().pixels(), &[255, 255, 255, 255]);
                assert!(textures.get(textures.error_color().unwrap()).unwrap().color_space() == jarvig_material::ColorSpace::Srgb);
                Ok::<(), &str>(())
            })
            .unwrap();
        assert!(tick.render_executed);
        assert_eq!(calls, 1);
        assert_eq!(engine.extraction_count(), 1);
        assert!(engine.extracted_this_frame());
        assert_eq!(engine.render_frame(), 1);
        assert_eq!(engine.last_visible_count(), 2);
        assert_eq!(engine.material_compile_count(), 1);
        assert_eq!(engine.last_light_count(), 3);
        assert_eq!(engine.directional_light_count(), 1);
        assert_eq!(engine.point_light_count(), 1);
        assert_eq!(engine.spot_light_count(), 1);
        assert_eq!(engine.world().environment_light_count(), 1);
        assert!(engine.world().environment().enabled);
        engine
            .run_frame(1.0 / 60.0, |_snapshot, _meshes, _materials, _textures| Ok::<(), &str>(()))
            .unwrap();
        assert_eq!(engine.extraction_count(), 2);
        assert_eq!(engine.render_frame(), 2);
    }
}

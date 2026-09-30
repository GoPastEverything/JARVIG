//! Native JARVIG core.
//!
//! The fixed-step clock matches the prototype in `engine/core`. Hosts still call
//! that prototype until a module has moved here and the same tests pass.
//! Plugins and game modules use the C ABI in `include/jarvig_core.h`, not the Rust ABI.

mod application;
mod bvh;
mod asset;
mod gltf;
mod gpu_scene;
mod clock;
mod input;
mod play;
mod settings;
mod entity;
mod environment;
mod ffi;
mod jobs;
mod json_lite;
mod level;
mod light;
mod material_set;
mod mesh;
mod meshlet;
mod meshlet_hierarchy;
mod meshlet_parents;
mod detail_provider;
mod microgeometry;
mod pick;
mod registry;
mod probe;
mod project;
mod quality;
mod reflect;
mod runtime;
mod scene;
mod shadow;
mod space;
mod texture;

pub use application::{ApplicationError, ApplicationPhase, GameApplication, RuntimeWorld};
pub use asset::{
    decode_derived_mesh, encode_derived_mesh, import_mesh_into_project, load_project_mesh_assets, proof_mesh_glb, AssetId, ImportedSurface,
    MeshAssetLibrary, MeshAssetRecord, ASSET_CATALOG_SCHEMA, MESH_IMPORTER_VERSION,
};
pub use gltf::{curved_prop_glb, import_gltf, ImportError};
pub use gpu_scene::{
    closest_meshlet_on_ray, frustum_cull_meshlets, occlusion_cull_meshlets, pack_gpu_scene, FrustumCull, GpuMeshletRecord, GpuSceneGeometry, GpuScenePack, GpuSceneStats,
    OcclusionCull,
};
pub use mesh::{mesh_from_surfaces, CanonicalSurface};
pub use meshlet_hierarchy::{
    build_cluster_hierarchy, leaves_under, select_cluster_cut, select_cluster_cut_for_pose, visit_leaves, ClusterCut, ClusterHierarchy, ClusterNode,
};
pub use jobs::{JobContext, JobDesc, JobId, JobManager, JobSnapshot, JobState};
pub use meshlet_parents::{
    build_parent_geometry, build_parent_geometry_with, load_parent_cache, parent_draw_indices, save_parent_cache, submission_for_cut, HierarchySubmission, ParentGeometry,
    ParentLevelStats, ParentRange, PARENT_BUILDER_VERSION,
};
pub use detail_provider::{public_detail, DisabledDetail, ProceduralMicrogeometry, ReferenceDetail};
pub use microgeometry::{
    linked_detail, build_procedural_microtriangles, build_procedural_microtriangles_cancellable, evaluate_detail, group_micro_spans,
    evaluate_procedural_microgeometry, probe_detail, projected_detail_px, surface_level, DetailDecision, DetailProbe, DetailProvider, DetailQuery, DetailRule, DetailSample,
    LocalPatch,
    MicroBudget, MicroMesh, MicroSpan, SurfaceAnchor, DETAIL_ERROR_THRESHOLD_PX,
    DETAIL_FEATURE_SIZE_M, PROCEDURAL_MICROGEOMETRY_DEFAULT,
};
pub use meshlet::{
    build_meshlets, decode_meshlets, encode_meshlets, meshlet_color, meshlet_draw, meshlets_cover_source, Meshlet, MeshletDraw, MeshletDrawRange, MeshletSet,
    MeshletStats, MESHLET_BUILDER_VERSION, MESHLET_MAX_TRIANGLES, MESHLET_MAX_VERTICES,
};
pub use input::{ActionState, ActionValue, InputDeviceState, InputMappingContext, PhysicalControl};
pub use play::PlayControl;
pub use settings::{load_game_settings, load_project_game_settings, GameSettings, PawnSelection, StartupCameraPolicy};
pub use clock::{ClockAdvance, ClockError, SimulationClock};
pub use entity::{AuthoringClass, ComponentMembership, EntityError, EntityHandle, EntityId, EntityOutlineInfo, EntityRegistry, EntityUuid};
pub use reflect::{
    find_field, find_type, format_f64, format_quat, format_vec3, type_registry, AuthoringError, AuthoringResult, EntityInspection,
    ComponentMultiplicity, FieldId, FieldInfo, InspectedField, InspectedSection, PropertyValue, TypeId, TypeInfo, ValueKind, FIELD_CAPTURE_STATE, FIELD_COLOR,
    FIELD_CASCADE_COUNT, FIELD_CASCADE_DISTRIBUTION, FIELD_CAST_SHADOWS, FIELD_ENABLED, FIELD_INNER_CONE, FIELD_INTENSITY, FIELD_LOCAL_ROTATION,
    FIELD_LOCAL_TRANSLATION, FIELD_LOWER_COLOR, FIELD_MATERIAL_SLOT, FIELD_NAME, FIELD_OUTER_CONE, FIELD_PARENT, FIELD_PRIORITY, FIELD_RADIUS,
    FIELD_RANGE, FIELD_RECEIVE_SHADOWS, FIELD_RESOLUTION, FIELD_SHADOW_BIAS, FIELD_SHADOW_DISTANCE, FIELD_SHADOW_FILTER, FIELD_SHADOW_NORMAL_BIAS,
    FIELD_CLEAR_POLICY, FIELD_COMPONENT_STACK, FIELD_FAR_PLANE, FIELD_METALLIC_FACTOR, FIELD_NEAR_PLANE, FIELD_OBJECT_SCALE, FIELD_ORTHO_HEIGHT, FIELD_PROJECTION, FIELD_VERTICAL_FOV, FIELD_VIEWPORT, FIELD_NORMAL_SCALE, FIELD_ROUGHNESS_FACTOR, FIELD_SHADOW_RESOLUTION, FIELD_SHADOW_SLOPE_BIAS, FIELD_SURFACE, FIELD_UPDATE_MODE, FIELD_UPPER_COLOR, FIELD_UUID, FIELD_UV_SCALE, FIELD_VISIBLE, TYPE_CAMERA, TYPE_COMPONENT_STACK, TYPE_DIRECTIONAL_LIGHT, TYPE_FREE_FLY, TYPE_PAWN,
    TYPE_ENTITY, TYPE_ENVIRONMENT, TYPE_MESH_RENDERER, TYPE_POINT_LIGHT,
    TYPE_REFLECTION_PROBE, TYPE_REGISTRY_VERSION, TYPE_SPATIAL_FRAME, TYPE_SPOT_LIGHT,
};
pub use pick::{perspective_ray, pick_snapshot, pick_snapshot_timed, PickHit, PickRay, PickTimings};
pub use material_set::{
    classify_material_file, engine_normal_convention, is_material_sidecar, negate_normal_green, pack_orm_rgba8, role_color_space, role_semantic,
    select_normal, MaterialMapRole, NormalConvention,
};
pub use mesh::{
    create_mesh, cube_mesh, emissive_panel_mesh, far_triangle_mesh, flat_sphere_mesh, floor_mesh, near_triangle_mesh, sphere_mesh,
    Aabb,
    BoundingSphere, LocalBounds, Mesh, MeshDesc,
    MeshError, MeshId, MeshIndexFormat, MeshLibrary, MeshTopology, MeshVertexAttribute, MeshVertexFormat, Submesh,
    SubmeshDesc, VertexStreamDesc,
};
pub use runtime::{Profile, Runtime, RuntimeError, RuntimeTick};
pub use environment::{
    environment_diffuse_for, environment_specular_for, plus_z_normal, visible_side_normal, EnvironmentLight, GpuEnvironmentPacket,
    BOOTSTRAP_ENVIRONMENT_INTENSITY, BOOTSTRAP_LOWER_HEMISPHERE_LINEAR, BOOTSTRAP_UPPER_HEMISPHERE_LINEAR,
};
pub use quality::{RenderBudget, RenderQuality};
pub use probe::{
    indirect_diffuse_direction, reflection_cube_faces, reflection_cube_texel_direction, reflection_probe_center,
    reflection_probe_mip_count_for, reflection_probe_resolution_supported, ProbeUpdatePolicy,
    reflection_probe_ggx_direction,
    reflection_probe_budget_resolution, reflection_probe_influence, reflection_probe_lod, reflection_probe_packet,
    reflection_probe_prefilter_roughness, reflection_probe_prefilter_sample_count, reflection_probe_weight,
    rotation_looking_toward, select_reflection_probe, GpuReflectionProbePacket, ProbeId, RenderReflectionProbe, BOOTSTRAP_PROBE_INTENSITY,
    BOOTSTRAP_PROBE_LOCAL_M, BOOTSTRAP_PROBE_PRIORITY, BOOTSTRAP_PROBE_RADIUS_M, INDIRECT_DIFFUSE_SAMPLES, REFLECTION_PROBE_MIP_COUNT,
    REFLECTION_PROBE_PREFILTER_SAMPLES,
    REFLECTION_PROBE_RESOLUTION,
};
pub use light::{
    emission_forward, finite_color, finite_intensity, finite_range, render_light_record, rotation_emitting_toward, GpuLightRecord,
    LightId, LightKind, RenderLight, SpotCone,
};
pub use shadow::{
    clip_matrix_for_camera, directional_shadow_far_m, directional_shadow_view_proj, fit_directional_cascades, light_relative_point,
    orthographic_reverse_z, perspective_reverse_z, point_shadow_face_view_proj, practical_splits, receiver_is_shadowed, shadow_ndc_orthographic,
    shadow_ndc_perspective, spot_shadow_view_proj, shadow_bias_ndc, shadow_receiver_bias, CascadeSlice, DirectionalCascadeSet, GpuShadowRecord,
    LightShadowSettings, ShadowMapClass, CASCADE_BLEND, CASCADE_RESOLUTION, CONTACT_SHADOW_DISTANCE_M, DEFAULT_CASCADE_COUNT,
    DEFAULT_CASCADE_LAMBDA, DIRECTIONAL_ATLAS_RESOLUTION, DIRECTIONAL_HALF_EXTENT_M, MAX_SHADOW_CASCADES, POINT_SHADOW_RESOLUTION,
    PUNCTUAL_LIGHT_RADIUS_M, SHADOW_CASTER_TRIANGLE_LIMIT, SHADOW_DEPTH_BIAS, SHADOW_DEPTH_BIAS_M, SHADOW_FAR_M, SHADOW_MAP_RESOLUTION, SHADOW_NEAR_M, SHADOW_NORMAL_BIAS_M,
    SHADOW_PASS_BUDGET, SHADOW_PCF_RADIUS, SHADOW_SLOPE_BIAS, SHADOW_SLOPE_BIAS_M, SUN_ANGULAR_TAN, casts_into_shadow_map,
};
pub use level::{
    parse_level, lighting_lab_level, CameraRecord, ComponentRecord, EntityRecord, LevelDocument, LevelError, LightRecord, MaterialAssetRef, MaterialScheme, MeshAssetRef,
    ProbeRecord, WorldSettingsRecord, LEVEL_CAMERA_VERSION, LEVEL_FORMAT_VERSION, LEVEL_SCHEMA, LIGHTING_LAB_LEVEL_UUID,
};
pub use registry::{
    model_subtype, reconcile_asset_registry, texture_subtype, thumbnail_rgba, AssetRegistry, RegistryAsset, RegistryRoots, REGISTRY_SCHEMA,
};
pub use project::{
    autosave_path, create_project_directories, load_level_file, load_project_file, parse_project, save_atomic, save_level_atomic,
    save_project_atomic, ProjectDocument, ProjectError, LIGHTING_LAB_PROJECT_UUID, PROJECT_FORMAT_VERSION, PROJECT_SCHEMA,
};
pub use scene::{
    instance_gpu_transforms, CameraId, ComponentBinding, ComponentQueryHit, ComponentRole, EntityCapabilities, EntityFocus, EntityOwnership, ExtractedCamera, ExtractedGameCamera, FocusError, MaterialSlotBinding, ObjectId,
    RenderFrameId, RenderInstance, RenderInstanceId, RenderSceneSnapshot, SceneWorld,
};
pub use jarvig_material::{ColorSpace, MaterialInstanceId};
pub use texture::{
    bootstrap_color_textures, bootstrap_pbr_textures, checkerboard, downsample_long_side, error_color_texture, full_mip_count, linear_channel_to_srgb_byte,
    solid, srgb_byte_to_linear, srgb_pixel, MipContent, Texture, TextureError, TextureLibrary,
};
pub use jarvig_material::{decode_normal, shade_normal, Mat3 as TangentMat3};
pub use space::{
    bootstrap_shared_world, bootstrap_triangle_scene, camera_relative_f32, render_transforms, BootstrapScene, Camera,
    FrameGraph, FrameId, SharedBootstrap,
    GpuTransforms, HighPrecisionPose, Mat4, Quat, ResolvedPose, SpaceError, Vec3, BOOTSTRAP_ROOT_M, DEPTH_CLEAR,
};

#[cfg(test)]
mod boundary_tests {
    #[test]
    fn core_crate_does_not_depend_on_the_rhi_or_wgpu() {
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("jarvig_rhi"));
        assert!(!manifest.contains("wgpu"));
    }
}

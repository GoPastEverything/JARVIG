//! Semantic type metadata. Not `std::any::TypeId`, not a memory offset, and not a C ABI.
//!
//! Schema version 1 is independent of the public API version. There is no migrator yet.
//! The inspector, a future save file, and automation are supposed to share this description.

use crate::{EntityId, Quat, Vec3};

/// Independent of `JARVIG_API_VERSION`. Bump it when a field meaning changes.
pub const TYPE_REGISTRY_VERSION: u32 = 6;

/// JARVIG schema id. Stable for this registry version. Not a Rust `TypeId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// JARVIG field id. Not a Win32 control id, not a byte offset, and not a slot index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FieldId(pub u32);

pub const TYPE_ENTITY: TypeId = TypeId(1);
pub const TYPE_SPATIAL_FRAME: TypeId = TypeId(2);
pub const TYPE_DIRECTIONAL_LIGHT: TypeId = TypeId(3);
pub const TYPE_POINT_LIGHT: TypeId = TypeId(4);
pub const TYPE_SPOT_LIGHT: TypeId = TypeId(5);
pub const TYPE_REFLECTION_PROBE: TypeId = TypeId(6);
pub const TYPE_ENVIRONMENT: TypeId = TypeId(7);
pub const TYPE_MESH_RENDERER: TypeId = TypeId(8);
pub const TYPE_COMPONENT_STACK: TypeId = TypeId(9);
pub const TYPE_CAMERA: TypeId = TypeId(10);
pub const TYPE_PAWN: TypeId = TypeId(11);
pub const TYPE_FREE_FLY: TypeId = TypeId(12);
pub const TYPE_JOINT: TypeId = TypeId(13);
pub const TYPE_TERRAIN: TypeId = TypeId(14);

pub const FIELD_NAME: FieldId = FieldId(1);
pub const FIELD_UUID: FieldId = FieldId(2);
pub const FIELD_PARENT: FieldId = FieldId(3);
pub const FIELD_LOCAL_TRANSLATION: FieldId = FieldId(4);
pub const FIELD_LOCAL_ROTATION: FieldId = FieldId(5);
pub const FIELD_COLOR: FieldId = FieldId(10);
pub const FIELD_INTENSITY: FieldId = FieldId(11);
pub const FIELD_RANGE: FieldId = FieldId(12);
pub const FIELD_ENABLED: FieldId = FieldId(13);
pub const FIELD_INNER_CONE: FieldId = FieldId(14);
pub const FIELD_OUTER_CONE: FieldId = FieldId(15);
pub const FIELD_RADIUS: FieldId = FieldId(16);
pub const FIELD_PRIORITY: FieldId = FieldId(17);
pub const FIELD_UPDATE_MODE: FieldId = FieldId(18);
pub const FIELD_CAPTURE_STATE: FieldId = FieldId(19);
pub const FIELD_RESOLUTION: FieldId = FieldId(20);
pub const FIELD_UPPER_COLOR: FieldId = FieldId(21);
pub const FIELD_LOWER_COLOR: FieldId = FieldId(22);
pub const FIELD_SURFACE: FieldId = FieldId(23);
pub const FIELD_MATERIAL_SLOT: FieldId = FieldId(24);
pub const FIELD_CAST_SHADOWS: FieldId = FieldId(25);
pub const FIELD_RECEIVE_SHADOWS: FieldId = FieldId(26);
pub const FIELD_SHADOW_RESOLUTION: FieldId = FieldId(27);
pub const FIELD_SHADOW_BIAS: FieldId = FieldId(28);
pub const FIELD_SHADOW_NORMAL_BIAS: FieldId = FieldId(29);
pub const FIELD_SHADOW_SLOPE_BIAS: FieldId = FieldId(30);
pub const FIELD_SHADOW_FILTER: FieldId = FieldId(31);
pub const FIELD_SHADOW_DISTANCE: FieldId = FieldId(32);
pub const FIELD_CASCADE_COUNT: FieldId = FieldId(33);
pub const FIELD_CASCADE_DISTRIBUTION: FieldId = FieldId(34);
pub const FIELD_UV_SCALE: FieldId = FieldId(35);
pub const FIELD_ROUGHNESS_FACTOR: FieldId = FieldId(36);
pub const FIELD_METALLIC_FACTOR: FieldId = FieldId(37);
pub const FIELD_NORMAL_SCALE: FieldId = FieldId(38);
pub const FIELD_COMPONENT_STACK: FieldId = FieldId(39);
pub const FIELD_PROJECTION: FieldId = FieldId(40);
pub const FIELD_VERTICAL_FOV: FieldId = FieldId(41);
pub const FIELD_ORTHO_HEIGHT: FieldId = FieldId(42);
pub const FIELD_NEAR_PLANE: FieldId = FieldId(43);
pub const FIELD_FAR_PLANE: FieldId = FieldId(44);
pub const FIELD_VIEWPORT: FieldId = FieldId(45);
pub const FIELD_CLEAR_POLICY: FieldId = FieldId(46);
pub const FIELD_OBJECT_SCALE: FieldId = FieldId(47);
pub const FIELD_VISIBLE: FieldId = FieldId(48);
pub const FIELD_JOINT_KIND: FieldId = FieldId(49);
pub const FIELD_JOINT_REST_TRANSLATION: FieldId = FieldId(50);
pub const FIELD_JOINT_REST_ROTATION: FieldId = FieldId(51);
pub const FIELD_JOINT_HINGE_MIN: FieldId = FieldId(52);
pub const FIELD_JOINT_HINGE_MAX: FieldId = FieldId(53);
pub const FIELD_JOINT_SWING: FieldId = FieldId(54);
pub const FIELD_JOINT_TWIST_MIN: FieldId = FieldId(55);
pub const FIELD_JOINT_TWIST_MAX: FieldId = FieldId(56);
pub const FIELD_JOINT_PRIMARY_MIN: FieldId = FieldId(57);
pub const FIELD_JOINT_PRIMARY_MAX: FieldId = FieldId(58);
pub const FIELD_JOINT_SECONDARY_MIN: FieldId = FieldId(59);
pub const FIELD_JOINT_SECONDARY_MAX: FieldId = FieldId(60);
pub const FIELD_JOINT_LINEAR_MIN: FieldId = FieldId(61);
pub const FIELD_JOINT_LINEAR_MAX: FieldId = FieldId(62);
pub const FIELD_JOINT_STIFFNESS: FieldId = FieldId(63);
pub const FIELD_JOINT_DAMPING: FieldId = FieldId(64);
pub const FIELD_JOINT_AXIS: FieldId = FieldId(65);
pub const FIELD_JOINT_SECONDARY_AXIS: FieldId = FieldId(66);
pub const FIELD_TERRAIN_WIDTH: FieldId = FieldId(67);
pub const FIELD_TERRAIN_DEPTH: FieldId = FieldId(68);
pub const FIELD_TERRAIN_SPACING: FieldId = FieldId(69);
pub const FIELD_TERRAIN_CHUNK: FieldId = FieldId(70);
pub const FIELD_TERRAIN_HEIGHT: FieldId = FieldId(71);
pub const FIELD_TERRAIN_HEIGHT_MIN: FieldId = FieldId(72);
pub const FIELD_TERRAIN_HEIGHT_MAX: FieldId = FieldId(73);
pub const FIELD_TERRAIN_COLLISION: FieldId = FieldId(74);
pub const FIELD_TERRAIN_LOD: FieldId = FieldId(75);
pub const FIELD_TERRAIN_MATERIAL: FieldId = FieldId(76);
pub const FIELD_TERRAIN_EINSTEIN: FieldId = FieldId(77);
pub const FIELD_TERRAIN_SEED: FieldId = FieldId(78);
pub const FIELD_TERRAIN_DENSITY: FieldId = FieldId(79);
pub const FIELD_TERRAIN_DISPLACEMENT: FieldId = FieldId(80);
pub const FIELD_TERRAIN_ERROR: FieldId = FieldId(81);
pub const FIELD_TERRAIN_DISTANCE: FieldId = FieldId(82);
pub const FIELD_TERRAIN_CLASS: FieldId = FieldId(83);
pub const FIELD_TERRAIN_CLIFF: FieldId = FieldId(84);
pub const FIELD_TERRAIN_DEBUG: FieldId = FieldId(85);
pub const FIELD_TERRAIN_DEBUG_COLORS: FieldId = FieldId(86);
pub const FIELD_TERRAIN_EINSTEIN_COLLISION: FieldId = FieldId(87);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    String,
    EntityUuid,
    OptionalEntityUuid,
    Float64,
    Vec3F64,
    QuatF64,
    Bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldInfo {
    pub id: FieldId,
    pub canonical_name: &'static str,
    pub display_name: &'static str,
    pub kind: ValueKind,
    pub readable: bool,
    pub editable: bool,
    pub group: &'static str,
    pub hint: &'static str,
    pub units: &'static str,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub precision: u8,
    /// Click and keyboard step. Zero means the widget does not scrub.
    pub step: f64,
    /// Empty when the field is not an asset reference. `Mesh` and `Material` are the inspector seam.
    pub asset_type: &'static str,
    /// Empty when the field is not a choice. The inspector builds a list from this, not a per-type window.
    pub enum_values: &'static [&'static str],
}

/// How many of this component one entity may own. Not a global rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentMultiplicity {
    /// `AddComponent` rejects this type. Entity and the stack view are not components.
    NotAComponent,
    One,
    Many { max: Option<u32> },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeInfo {
    pub id: TypeId,
    pub canonical_name: &'static str,
    pub display_name: &'static str,
    pub version: u32,
    pub fields: &'static [FieldInfo],
    pub multiplicity: ComponentMultiplicity,
    pub requires: &'static [TypeId],
    pub rejects: &'static [TypeId],
}

const NO_TYPES: &[TypeId] = &[];
const NO_FIELDS: &[FieldInfo] = &[];
const NO_ENUM: &[&str] = &[];
const REQUIRES_PAWN: &[TypeId] = &[TYPE_PAWN];
const REQUIRES_TRANSFORM: &[TypeId] = &[TYPE_SPATIAL_FRAME];
const REJECTS_TRANSFORM: &[TypeId] = &[TYPE_SPATIAL_FRAME];
const MESH_PRIMITIVES: &[&str] = &["Cube", "Sphere", "Plane"];
const CAMERA_PROJECTIONS: &[&str] = &["Perspective", "Orthographic"];
const JOINT_KINDS: &[&str] = &["Fixed", "Hinge", "Ball", "Universal", "Prismatic"];
const PROBE_POLICIES: &[&str] = &["Static", "On Demand", "On Transform", "On Lighting", "Time Sliced"];
const PROBE_RESOLUTIONS: &[&str] = &["32", "64", "128", "256"];
const SHADOW_RESOLUTIONS: &[&str] = &["Default", "256", "512", "1024", "2048", "4096"];

const ENTITY_FIELDS: &[FieldInfo] = &[
    FieldInfo {
        id: FIELD_NAME,
        canonical_name: "name",
        display_name: "Name",
        kind: ValueKind::String,
        readable: true,
        editable: true,
        group: "Entity",
        hint: "text",
        units: "",
        minimum: None,
        maximum: None,
        precision: 0,
        step: 0.0,
        asset_type: "",
        enum_values: NO_ENUM,
    },
    FieldInfo {
        id: FIELD_UUID,
        canonical_name: "uuid",
        display_name: "UUID",
        kind: ValueKind::EntityUuid,
        readable: true,
        editable: false,
        group: "Entity",
        hint: "readonly-text",
        units: "",
        minimum: None,
        maximum: None,
        precision: 0,
        step: 0.0,
        asset_type: "",
        enum_values: NO_ENUM,
    },
    FieldInfo {
        id: FIELD_PARENT,
        canonical_name: "parent",
        display_name: "Parent",
        kind: ValueKind::OptionalEntityUuid,
        readable: true,
        editable: false,
        group: "Entity",
        hint: "readonly-text",
        units: "",
        minimum: None,
        maximum: None,
        precision: 0,
        step: 0.0,
        asset_type: "",
        enum_values: NO_ENUM,
    },
];

const FRAME_FIELDS: &[FieldInfo] = &[
    FieldInfo {
        id: FIELD_LOCAL_TRANSLATION,
        canonical_name: "local_translation",
        display_name: "Location",
        kind: ValueKind::Vec3F64,
        readable: true,
        editable: true,
        group: "Transform",
        hint: "vector3",
        units: "m",
        minimum: None,
        maximum: None,
        precision: 3,
        step: 0.01,
        asset_type: "",
        enum_values: NO_ENUM,
    },
    FieldInfo {
        id: FIELD_LOCAL_ROTATION,
        canonical_name: "local_rotation",
        display_name: "Rotation",
        kind: ValueKind::QuatF64,
        readable: true,
        editable: true,
        group: "Transform",
        hint: "euler-degrees",
        units: "deg",
        minimum: None,
        maximum: None,
        precision: 2,
        step: 1.0,
        asset_type: "",
        enum_values: NO_ENUM,
    },
];

fn component_field(
    id: FieldId,
    canonical: &'static str,
    display: &'static str,
    kind: ValueKind,
    editable: bool,
    group: &'static str,
    units: &'static str,
) -> FieldInfo {
    let hint = match kind {
        ValueKind::Vec3F64 => "vector3",
        ValueKind::QuatF64 => "quaternion",
        ValueKind::Bool => "bool",
        _ if editable => "text",
        _ => "readonly-text",
    };
    let (minimum, maximum) = match id {
        FIELD_ROUGHNESS_FACTOR | FIELD_METALLIC_FACTOR | FIELD_NORMAL_SCALE | FIELD_UV_SCALE => (Some(0.0), None),
        FIELD_RADIUS => (Some(0.01), None),
        _ => (None, None),
    };
    let precision = match id {
        FIELD_ROUGHNESS_FACTOR | FIELD_METALLIC_FACTOR | FIELD_NORMAL_SCALE | FIELD_UV_SCALE | FIELD_VERTICAL_FOV | FIELD_INTENSITY => 2,
        _ => 3,
    };
    let step = match id {
        FIELD_VERTICAL_FOV => 1.0,
        FIELD_INTENSITY | FIELD_RANGE | FIELD_RADIUS => 0.1,
        _ => 0.01,
    };
    FieldInfo {
        id,
        canonical_name: canonical,
        display_name: display,
        kind,
        readable: true,
        editable,
        group,
        hint,
        units,
        minimum,
        maximum,
        precision,
        step,
        asset_type: "",
        enum_values: NO_ENUM,
    }
}

fn with_asset(mut field: FieldInfo, asset_type: &'static str) -> FieldInfo {
    field.asset_type = asset_type;
    field
}

fn with_choices(mut field: FieldInfo, enum_values: &'static [&'static str]) -> FieldInfo {
    field.enum_values = enum_values;
    field
}

fn shadow_fields(group: &'static str, distance_label: &'static str, cascades: bool) -> Vec<FieldInfo> {
    let mut fields = vec![
        component_field(FIELD_CAST_SHADOWS, "cast_shadows", "Cast Shadows", ValueKind::Bool, true, group, ""),
        with_choices(component_field(FIELD_SHADOW_RESOLUTION, "shadow_resolution", "Shadow Resolution", ValueKind::Float64, true, group, "px"), SHADOW_RESOLUTIONS),
        component_field(FIELD_SHADOW_BIAS, "depth_bias_m", "Bias", ValueKind::Float64, true, group, "m"),
        component_field(FIELD_SHADOW_SLOPE_BIAS, "slope_bias_m", "Slope Bias", ValueKind::Float64, true, group, "m"),
        component_field(FIELD_SHADOW_NORMAL_BIAS, "normal_bias_m", "Normal Bias", ValueKind::Float64, true, group, "m"),
        component_field(FIELD_SHADOW_FILTER, "filter_radius", "Filter Radius", ValueKind::Float64, true, group, "texel"),
        component_field(FIELD_SHADOW_DISTANCE, "shadow_distance_m", distance_label, ValueKind::Float64, true, group, "m"),
    ];
    if cascades {
        fields.push(component_field(FIELD_CASCADE_COUNT, "cascade_count", "Cascade Count", ValueKind::Float64, true, group, ""));
        fields.push(component_field(
            FIELD_CASCADE_DISTRIBUTION,
            "cascade_distribution",
            "Cascade Distribution",
            ValueKind::Float64,
            true,
            group,
            "0 uniform, 1 log",
        ));
    }
    fields
}

fn directional_fields() -> &'static [FieldInfo] {
    let mut fields = vec![
        component_field(FIELD_COLOR, "color", "Color", ValueKind::Vec3F64, true, "Directional Light", "linear"),
        component_field(FIELD_INTENSITY, "intensity", "Intensity", ValueKind::Float64, true, "Directional Light", "lux"),
        component_field(FIELD_ENABLED, "enabled", "Enabled", ValueKind::Bool, true, "Directional Light", ""),
    ];
    fields.extend(shadow_fields("Directional Light", "Maximum Shadow Distance", true));
    Box::leak(fields.into_boxed_slice())
}

fn point_fields() -> &'static [FieldInfo] {
    let mut fields = vec![
        component_field(FIELD_COLOR, "color", "Color", ValueKind::Vec3F64, true, "Point Light", "linear"),
        component_field(FIELD_INTENSITY, "intensity", "Intensity", ValueKind::Float64, true, "Point Light", "cd"),
        component_field(FIELD_RANGE, "range", "Range", ValueKind::Float64, true, "Point Light", "m"),
        component_field(FIELD_ENABLED, "enabled", "Enabled", ValueKind::Bool, true, "Point Light", ""),
    ];
    fields.extend(shadow_fields("Point Light", "Shadow Distance", false));
    Box::leak(fields.into_boxed_slice())
}

fn spot_fields() -> &'static [FieldInfo] {
    let mut fields = vec![
        component_field(FIELD_COLOR, "color", "Color", ValueKind::Vec3F64, true, "Spot Light", "linear"),
        component_field(FIELD_INTENSITY, "intensity", "Intensity", ValueKind::Float64, true, "Spot Light", "cd"),
        component_field(FIELD_RANGE, "range", "Range", ValueKind::Float64, true, "Spot Light", "m"),
        component_field(FIELD_INNER_CONE, "inner_cone", "Inner Cone", ValueKind::Float64, true, "Spot Light", "rad"),
        component_field(FIELD_OUTER_CONE, "outer_cone", "Outer Cone", ValueKind::Float64, true, "Spot Light", "rad"),
        component_field(FIELD_ENABLED, "enabled", "Enabled", ValueKind::Bool, true, "Spot Light", ""),
    ];
    fields.extend(shadow_fields("Spot Light", "Shadow Distance", false));
    Box::leak(fields.into_boxed_slice())
}

fn probe_fields() -> &'static [FieldInfo] {
    Box::leak(Box::new([
        component_field(FIELD_RADIUS, "radius", "Influence Size", ValueKind::Float64, true, "Reflection Probe", "m"),
        component_field(FIELD_PRIORITY, "priority", "Priority", ValueKind::Float64, true, "Reflection Probe", ""),
        component_field(FIELD_INTENSITY, "intensity", "Intensity", ValueKind::Float64, true, "Reflection Probe", ""),
        component_field(FIELD_ENABLED, "enabled", "Enabled", ValueKind::Bool, true, "Reflection Probe", ""),
        with_choices(component_field(FIELD_UPDATE_MODE, "update_mode", "Update Policy", ValueKind::String, true, "Reflection Probe", ""), PROBE_POLICIES),
        component_field(FIELD_CAPTURE_STATE, "capture_state", "Capture", ValueKind::String, false, "Reflection Probe", ""),
        with_choices(component_field(FIELD_RESOLUTION, "resolution", "Resolution", ValueKind::Float64, true, "Reflection Probe", "px"), PROBE_RESOLUTIONS),
    ]))
}

fn environment_fields() -> &'static [FieldInfo] {
    Box::leak(Box::new([
        component_field(FIELD_UPPER_COLOR, "upper", "Upper Hemisphere", ValueKind::Vec3F64, true, "Environment", "linear"),
        component_field(FIELD_LOWER_COLOR, "lower", "Lower Hemisphere", ValueKind::Vec3F64, true, "Environment", "linear"),
        component_field(FIELD_INTENSITY, "intensity", "Intensity", ValueKind::Float64, true, "Environment", ""),
        component_field(FIELD_ENABLED, "enabled", "Enabled", ValueKind::Bool, true, "Environment", ""),
    ]))
}

fn stack_fields() -> &'static [FieldInfo] {
    Box::leak(Box::new([component_field(
        FIELD_COMPONENT_STACK,
        "stack",
        "Stack",
        ValueKind::String,
        false,
        "Components",
        "",
    )]))
}

fn mesh_fields() -> &'static [FieldInfo] {
    Box::leak(Box::new([
        with_choices(with_asset(component_field(FIELD_SURFACE, "surface", "Mesh", ValueKind::String, false, "Mesh Renderer", ""), "Mesh"), MESH_PRIMITIVES),
        component_field(FIELD_OBJECT_SCALE, "scale", "Scale", ValueKind::Vec3F64, true, "Transform", ""),
        with_asset(component_field(FIELD_MATERIAL_SLOT, "material", "Element 0", ValueKind::String, false, "Materials", ""), "Material"),
        component_field(FIELD_UV_SCALE, "uv_scale", "UV Scale", ValueKind::Float64, true, "Material Instance", ""),
        component_field(FIELD_ROUGHNESS_FACTOR, "roughness", "Roughness Mult", ValueKind::Float64, true, "Material Instance", ""),
        component_field(FIELD_METALLIC_FACTOR, "metallic", "Metallic Mult", ValueKind::Float64, true, "Material Instance", ""),
        component_field(FIELD_NORMAL_SCALE, "normal_scale", "Normal Strength", ValueKind::Float64, true, "Material Instance", ""),
        component_field(FIELD_VISIBLE, "visible", "Visible", ValueKind::Bool, true, "Mesh Renderer", ""),
        component_field(FIELD_CAST_SHADOWS, "cast_shadows", "Cast Shadows", ValueKind::Bool, true, "Mesh Renderer", ""),
        component_field(FIELD_RECEIVE_SHADOWS, "receive_shadows", "Receive Shadows", ValueKind::Bool, true, "Mesh Renderer", ""),
    ]))
}

fn camera_fields() -> &'static [FieldInfo] {
    Box::leak(Box::new([
        component_field(FIELD_ENABLED, "enabled", "Enabled", ValueKind::Bool, true, "Camera", ""),
        with_choices(component_field(FIELD_PROJECTION, "projection", "Projection", ValueKind::String, true, "Camera", ""), CAMERA_PROJECTIONS),
        component_field(FIELD_VERTICAL_FOV, "vertical_fov_deg", "Vertical FOV", ValueKind::Float64, true, "Camera", "deg"),
        component_field(FIELD_ORTHO_HEIGHT, "ortho_height_m", "Orthographic Height", ValueKind::Float64, true, "Camera", "m"),
        component_field(FIELD_NEAR_PLANE, "near_m", "Near", ValueKind::Float64, true, "Camera", "m"),
        component_field(FIELD_FAR_PLANE, "far_m", "Far", ValueKind::Float64, true, "Camera", "m"),
        component_field(FIELD_PRIORITY, "priority", "Priority", ValueKind::Float64, true, "Camera", ""),
        component_field(FIELD_VIEWPORT, "viewport", "Viewport", ValueKind::String, true, "Camera", ""),
        component_field(FIELD_CLEAR_POLICY, "clear", "Clear", ValueKind::String, false, "Camera", ""),
    ]))
}

fn euler_field(mut field: FieldInfo) -> FieldInfo {
    field.hint = "euler-degrees";
    field.units = "deg";
    field.precision = 2;
    field.step = 1.0;
    field
}

fn joint_fields() -> &'static [FieldInfo] {
    Box::leak(Box::new([
        with_choices(component_field(FIELD_JOINT_KIND, "kind", "Kind", ValueKind::String, true, "Joint", ""), JOINT_KINDS),
        component_field(FIELD_JOINT_REST_TRANSLATION, "rest_translation", "Rest Location", ValueKind::Vec3F64, true, "Joint", "m"),
        euler_field(component_field(FIELD_JOINT_REST_ROTATION, "rest_rotation", "Rest Rotation", ValueKind::QuatF64, true, "Joint", "deg")),
        component_field(FIELD_JOINT_AXIS, "axis", "Axis", ValueKind::Vec3F64, true, "Joint", ""),
        component_field(FIELD_JOINT_SECONDARY_AXIS, "secondary_axis", "Secondary Axis", ValueKind::Vec3F64, true, "Joint", ""),
        component_field(FIELD_JOINT_HINGE_MIN, "hinge_min_deg", "Hinge Min", ValueKind::Float64, true, "Limits", "deg"),
        component_field(FIELD_JOINT_HINGE_MAX, "hinge_max_deg", "Hinge Max", ValueKind::Float64, true, "Limits", "deg"),
        component_field(FIELD_JOINT_SWING, "swing_deg", "Swing Cone", ValueKind::Float64, true, "Limits", "deg"),
        component_field(FIELD_JOINT_TWIST_MIN, "twist_min_deg", "Twist Min", ValueKind::Float64, true, "Limits", "deg"),
        component_field(FIELD_JOINT_TWIST_MAX, "twist_max_deg", "Twist Max", ValueKind::Float64, true, "Limits", "deg"),
        component_field(FIELD_JOINT_PRIMARY_MIN, "primary_min_deg", "Primary Min", ValueKind::Float64, true, "Limits", "deg"),
        component_field(FIELD_JOINT_PRIMARY_MAX, "primary_max_deg", "Primary Max", ValueKind::Float64, true, "Limits", "deg"),
        component_field(FIELD_JOINT_SECONDARY_MIN, "secondary_min_deg", "Secondary Min", ValueKind::Float64, true, "Limits", "deg"),
        component_field(FIELD_JOINT_SECONDARY_MAX, "secondary_max_deg", "Secondary Max", ValueKind::Float64, true, "Limits", "deg"),
        component_field(FIELD_JOINT_LINEAR_MIN, "linear_min_m", "Slide Min", ValueKind::Float64, true, "Limits", "m"),
        component_field(FIELD_JOINT_LINEAR_MAX, "linear_max_m", "Slide Max", ValueKind::Float64, true, "Limits", "m"),
        component_field(FIELD_JOINT_STIFFNESS, "stiffness", "Stiffness", ValueKind::Float64, true, "Joint", ""),
        component_field(FIELD_JOINT_DAMPING, "damping", "Damping", ValueKind::Float64, true, "Joint", ""),
    ]))
}

const TERRAIN_CLIFFS: &[&str] = &["Horizontal XZ", "Dominant Axis"];

fn terrain_fields() -> &'static [FieldInfo] {
    Box::leak(Box::new([
        component_field(FIELD_TERRAIN_WIDTH, "width_m", "Width", ValueKind::Float64, false, "Terrain", "m"),
        component_field(FIELD_TERRAIN_DEPTH, "depth_m", "Depth", ValueKind::Float64, false, "Terrain", "m"),
        component_field(FIELD_TERRAIN_SPACING, "spacing_m", "Meters Per Vertex", ValueKind::Float64, false, "Terrain", "m"),
        component_field(FIELD_TERRAIN_CHUNK, "chunk_m", "Chunk Size", ValueKind::Float64, false, "Terrain", "m"),
        component_field(FIELD_TERRAIN_HEIGHT, "height_m", "Height", ValueKind::Float64, false, "Terrain", "m"),
        component_field(FIELD_TERRAIN_HEIGHT_MIN, "height_min_m", "Height Min", ValueKind::Float64, true, "Terrain", "m"),
        component_field(FIELD_TERRAIN_HEIGHT_MAX, "height_max_m", "Height Max", ValueKind::Float64, true, "Terrain", "m"),
        component_field(FIELD_TERRAIN_COLLISION, "collision", "Collision", ValueKind::Bool, true, "Terrain", ""),
        component_field(FIELD_TERRAIN_LOD, "lod", "LOD", ValueKind::Bool, true, "Terrain", ""),
        component_field(FIELD_TERRAIN_MATERIAL, "material", "Material", ValueKind::String, false, "Terrain", ""),
        component_field(FIELD_TERRAIN_EINSTEIN, "einstein", "Einstein Detail", ValueKind::Bool, true, "Einstein Detail", ""),
        component_field(FIELD_TERRAIN_SEED, "einstein_seed", "Einstein Seed", ValueKind::Float64, true, "Einstein Detail", ""),
        component_field(FIELD_TERRAIN_DENSITY, "einstein_density", "Density", ValueKind::Float64, true, "Einstein Detail", ""),
        component_field(FIELD_TERRAIN_DISPLACEMENT, "einstein_displacement_m", "Maximum Displacement", ValueKind::Float64, true, "Einstein Detail", "m"),
        component_field(FIELD_TERRAIN_ERROR, "einstein_error_px", "Projected Error", ValueKind::Float64, true, "Einstein Detail", "px"),
        component_field(FIELD_TERRAIN_DISTANCE, "einstein_distance_m", "Detail Distance", ValueKind::Float64, true, "Einstein Detail", "m"),
        component_field(FIELD_TERRAIN_CLASS, "einstein_surface_class", "Surface Class", ValueKind::Float64, true, "Einstein Detail", ""),
        with_choices(component_field(FIELD_TERRAIN_CLIFF, "cliff", "Cliff Projection", ValueKind::String, true, "Einstein Detail", ""), TERRAIN_CLIFFS),
        component_field(FIELD_TERRAIN_DEBUG, "debug", "Debug Visualization", ValueKind::Bool, true, "Terrain", ""),
        component_field(FIELD_TERRAIN_DEBUG_COLORS, "einstein_debug_colors", "Debug Colors", ValueKind::Bool, true, "Einstein Detail", ""),
        component_field(FIELD_TERRAIN_EINSTEIN_COLLISION, "einstein_collision", "Einstein Collision", ValueKind::Bool, false, "Einstein Detail", ""),
    ]))
}

fn described(
    id: TypeId,
    canonical_name: &'static str,
    display_name: &'static str,
    fields: &'static [FieldInfo],
    multiplicity: ComponentMultiplicity,
    requires: &'static [TypeId],
    rejects: &'static [TypeId],
) -> TypeInfo {
    TypeInfo { id, canonical_name, display_name, version: 1, fields, multiplicity, requires, rejects }
}

fn registry() -> &'static [TypeInfo] {
    Box::leak(Box::new([
        described(TYPE_ENTITY, "entity", "Entity", ENTITY_FIELDS, ComponentMultiplicity::NotAComponent, NO_TYPES, NO_TYPES),
        described(TYPE_SPATIAL_FRAME, "spatial_frame", "Transform", FRAME_FIELDS, ComponentMultiplicity::One, NO_TYPES, NO_TYPES),
        described(TYPE_DIRECTIONAL_LIGHT, "directional_light", "Directional Light", directional_fields(), ComponentMultiplicity::One, REQUIRES_TRANSFORM, NO_TYPES),
        described(TYPE_POINT_LIGHT, "point_light", "Point Light", point_fields(), ComponentMultiplicity::One, REQUIRES_TRANSFORM, NO_TYPES),
        described(TYPE_SPOT_LIGHT, "spot_light", "Spot Light", spot_fields(), ComponentMultiplicity::One, REQUIRES_TRANSFORM, NO_TYPES),
        described(TYPE_REFLECTION_PROBE, "sphere_reflection_probe", "Sphere Reflection Probe", probe_fields(), ComponentMultiplicity::One, REQUIRES_TRANSFORM, NO_TYPES),
        described(TYPE_ENVIRONMENT, "environment", "Environment", environment_fields(), ComponentMultiplicity::One, NO_TYPES, REJECTS_TRANSFORM),
        described(TYPE_MESH_RENDERER, "mesh_renderer", "Mesh Renderer", mesh_fields(), ComponentMultiplicity::One, REQUIRES_TRANSFORM, NO_TYPES),
        described(TYPE_CAMERA, "camera", "Camera", camera_fields(), ComponentMultiplicity::One, REQUIRES_TRANSFORM, NO_TYPES),
        described(TYPE_PAWN, "pawn", "Pawn", NO_FIELDS, ComponentMultiplicity::One, REQUIRES_TRANSFORM, NO_TYPES),
        described(TYPE_FREE_FLY, "free_fly", "Free Fly", NO_FIELDS, ComponentMultiplicity::One, REQUIRES_PAWN, NO_TYPES),
        described(TYPE_JOINT, "joint", "Joint", joint_fields(), ComponentMultiplicity::One, REQUIRES_TRANSFORM, NO_TYPES),
        described(TYPE_TERRAIN, "terrain", "Terrain", terrain_fields(), ComponentMultiplicity::One, REQUIRES_TRANSFORM, NO_TYPES),
        described(TYPE_COMPONENT_STACK, "component_stack", "Components", stack_fields(), ComponentMultiplicity::NotAComponent, NO_TYPES, NO_TYPES),
    ]))
}

pub fn type_registry() -> &'static [TypeInfo] {
    use std::sync::OnceLock;
    static HELD: OnceLock<&'static [TypeInfo]> = OnceLock::new();
    HELD.get_or_init(registry)
}

pub fn find_type(id: TypeId) -> Option<&'static TypeInfo> {
    type_registry().iter().find(|info| info.id == id)
}

pub fn find_field(type_id: TypeId, field: FieldId) -> Option<&'static FieldInfo> {
    find_type(type_id)?.fields.iter().find(|info| info.id == field)
}

/// Semantic property value. Not JSON, not a window handle, and not a GPU type.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    String(String),
    F64(f64),
    Vec3(Vec3),
    Quat(Quat),
    Entity(EntityId),
    OptionalEntity(Option<EntityId>),
    Bool(bool),
}

impl PropertyValue {
    pub fn matches(&self, kind: ValueKind) -> bool {
        matches!(
            (self, kind),
            (Self::String(_), ValueKind::String)
                | (Self::F64(_), ValueKind::Float64)
                | (Self::Vec3(_), ValueKind::Vec3F64)
                | (Self::Quat(_), ValueKind::QuatF64)
                | (Self::Entity(_), ValueKind::EntityUuid)
                | (Self::OptionalEntity(_), ValueKind::OptionalEntityUuid)
                | (Self::Bool(_), ValueKind::Bool)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthoringResult {
    Applied,
    Unchanged,
    /// The new entity. Not the source, and not a subsystem id.
    Duplicated(EntityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthoringError {
    NotFound,
    FieldNotFound,
    ReadOnly,
    WrongType,
    InvalidValue,
    InvalidOperation,
    /// The entity exists, but this lifecycle operation is refused. World Settings.
    ProtectedEntity,
    /// The entity exists, but its component mix cannot be copied or removed safely.
    Unsupported,
}

impl std::fmt::Display for AuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "entity does not exist",
            Self::FieldNotFound => "field does not exist",
            Self::ReadOnly => "field is read only",
            Self::WrongType => "property value has the wrong type",
            Self::InvalidValue => "property value is invalid",
            Self::InvalidOperation => "property does not apply to this entity",
            Self::ProtectedEntity => "entity is protected",
            Self::Unsupported => "entity lifecycle is not supported",
        })
    }
}

/// One reflected field read. Derived data. Not a second world.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectedField {
    pub info: FieldInfo,
    pub value: PropertyValue,
    /// Presentation for labels that are not the raw value, such as a parent name.
    pub display: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InspectedSection {
    pub type_info: TypeInfo,
    pub fields: Vec<InspectedField>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityInspection {
    pub entity: EntityId,
    pub world_revision: u64,
    pub sections: Vec<InspectedSection>,
}

pub fn format_f64(value: f64) -> String {
    let text = format!("{value}");
    if text == "-0" { "0".to_string() } else { text }
}

pub fn format_vec3(value: Vec3) -> String {
    format!("{}, {}, {}", format_f64(value.x), format_f64(value.y), format_f64(value.z))
}

pub fn format_quat(value: Quat) -> String {
    format!(
        "{}, {}, {}, {}",
        format_f64(value.x),
        format_f64(value.y),
        format_f64(value.z),
        format_f64(value.w)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_are_deterministic_and_not_rust_type_ids() {
        assert_eq!(TYPE_REGISTRY_VERSION, 6);
        let terrain = find_type(TYPE_TERRAIN).unwrap();
        assert_eq!(terrain.canonical_name, "terrain");
        assert!(!find_field(TYPE_TERRAIN, FIELD_TERRAIN_WIDTH).unwrap().editable);
        assert!(!find_field(TYPE_TERRAIN, FIELD_TERRAIN_EINSTEIN_COLLISION).unwrap().editable);
        assert!(find_field(TYPE_TERRAIN, FIELD_TERRAIN_EINSTEIN).unwrap().editable);
        let types = type_registry();
        assert!(types.len() >= 2);
        for (index, info) in types.iter().enumerate() {
            assert!(types.iter().skip(index + 1).all(|other| other.id != info.id));
            assert!(!info.canonical_name.is_empty());
            let mut seen = Vec::new();
            for field in info.fields {
                assert!(!seen.contains(&field.id), "duplicate field");
                seen.push(field.id);
                assert!(field.readable);
            }
        }
        let entity = find_type(TYPE_ENTITY).unwrap();
        assert_eq!(entity.canonical_name, "entity");
        let name = find_field(TYPE_ENTITY, FIELD_NAME).unwrap();
        assert!(name.editable && name.kind == ValueKind::String);
        let uuid = find_field(TYPE_ENTITY, FIELD_UUID).unwrap();
        assert!(!uuid.editable && uuid.kind == ValueKind::EntityUuid);
        let parent = find_field(TYPE_ENTITY, FIELD_PARENT).unwrap();
        assert!(!parent.editable);
        let frame = find_type(TYPE_SPATIAL_FRAME).unwrap();
        assert_eq!(frame.canonical_name, "spatial_frame");
        let location = find_field(TYPE_SPATIAL_FRAME, FIELD_LOCAL_TRANSLATION).unwrap();
        assert!(location.editable && location.units == "m" && location.kind == ValueKind::Vec3F64);
        let rotation = find_field(TYPE_SPATIAL_FRAME, FIELD_LOCAL_ROTATION).unwrap();
        assert!(rotation.editable && rotation.kind == ValueKind::QuatF64 && rotation.hint == "euler-degrees" && rotation.units == "deg");
        let mesh = find_field(TYPE_MESH_RENDERER, FIELD_SURFACE).unwrap();
        assert_eq!(mesh.asset_type, "Mesh");
        assert_eq!(mesh.enum_values, ["Cube", "Sphere", "Plane"]);
        assert!(!mesh.editable);
        let material = find_field(TYPE_MESH_RENDERER, FIELD_MATERIAL_SLOT).unwrap();
        assert_eq!(material.asset_type, "Material");
        assert_eq!(material.group, "Materials");
        let roughness = find_field(TYPE_MESH_RENDERER, FIELD_ROUGHNESS_FACTOR).unwrap();
        assert_eq!(roughness.minimum, Some(0.0));
        assert_eq!(roughness.group, "Material Instance");
        assert!(find_field(TYPE_ENTITY, FIELD_LOCAL_TRANSLATION).is_none());
    }
}

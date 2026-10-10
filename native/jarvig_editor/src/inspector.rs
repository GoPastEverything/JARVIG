//! Derived inspector. Editor presentation of an engine inspection snapshot.
//!
//! Field identity is a JARVIG type id plus a field id. A Win32 control id is not
//! that identity. The window builds controls from [`plan`]; it does not special-case
//! each component with its own form. Multi-selection does not edit one entity silently.

use jarvig_core::{
    find_type, type_registry, AuthoringCapabilities, AuthoringClass, ComponentMultiplicity, ComponentRole, EntityInspection, EntityUuid, FieldId, PropertyValue, Quat, TypeId,
    ValueKind, FIELD_CAPTURE_STATE, FIELD_MATERIAL_SLOT, FIELD_NAME, FIELD_OBJECT_SCALE, FIELD_PARENT, FIELD_SURFACE, FIELD_UUID,
    TYPE_COMPONENT_STACK, TYPE_DIRECTIONAL_LIGHT, TYPE_ENTITY, TYPE_ENVIRONMENT, TYPE_FREE_FLY, TYPE_MESH_RENDERER, TYPE_PAWN, TYPE_PARAMETRIC_BLOCK, TYPE_POINT_LIGHT,
    TYPE_TERRAIN, TYPE_REFLECTION_PROBE, TYPE_SPATIAL_FRAME, TYPE_SPOT_LIGHT,
};
use jarvig_engine::EngineSession;

use crate::selection::{SelectionItem, SelectionService};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetKind {
    Text,
    Number,
    Check,
    Vector,
    Euler,
    Choice,
    Color,
    Readonly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectorCommand {
    RemoveComponent,
    ResetTransform,
    ResetMaterial,
    SetStartupCamera,
    Recapture,
    /// Writes every joint in the open world back to its rest pose.
    ResetPose,
    /// Creates one flat heightfield from the Land Mode draft. Not a component command.
    CreateTerrain,
    /// Size returns to 2 m. Position and material stay.
    ResetShape,
    /// Opens an extrude session on the selected face. A missing face uses +X.
    ExtrudeFace,
    /// Opens an inset session on the selected face. A missing face uses +X.
    InsetFace,
    /// Opens a bevel session. The chamfer is one amount for every edge.
    Bevel,
    /// Opens a round session on one semantic edge. The radius is the authored fillet.
    Round,
    /// Drops the open modeling preview and restores the solid.
    CancelModeling,
    /// Writes one history entry for the open modeling preview.
    ApplyModeling,
    /// Writes the open round. Labeled Apply. Bevel and the other tools stay on Done.
    ApplyRound,
    MirrorX,
    MirrorY,
    MirrorZ,
    /// The existing duplicate command. Not a second copy path.
    Duplicate,
    AlignToGround,
    SnapTranslation,
    /// Reopens the bevel session at the solid's current amount.
    EditBevel,
    /// Reopens the inset session for one face at that face's current distance.
    EditInset(u8),
    SelectionAuto,
    SelectionObject,
    SelectionFace,
    SelectionEdge,
    SelectionVertex,
    /// Edge loop through valence-4 vertices. A pole stops the walk.
    SelectLoop,
    /// Opposite edges across four-edge faces.
    SelectRing,
    /// The connected component of the current faces, edges, or vertices.
    SelectConnected,
    /// Boundary edges of the selected faces.
    SelectBoundary,
    /// One adjacency layer around the current set.
    SelectGrow,
    /// Drops the outer adjacency layer.
    SelectShrink,
    /// Moves both ends of the selected edge. The surrounding faces stay attached.
    MoveEdge,
    /// Adds one edge and the wall that joins it to the selected edge.
    ExtrudeEdge,
    /// Inserts the midpoint. The old edge id stays on the first half.
    SplitEdge,
    /// Moves the selected vertex. Loops that already contain it stay connected.
    MoveVertex,
    /// Replaces the selected quad with a 2 by 2 grid of real faces.
    SubdivideFace,
    /// Replaces the selected quad with a 4 by 4 grid of real faces.
    SubdivideFace4,
    /// Copies slot 0 into a new slot. Does not assign it.
    AddMaterialSlot,
    /// Copies the selection's material into the next slot and assigns only those faces.
    MakeUnique,
    /// Replaces the face selection with every live face on the current slot.
    SelectSlotFaces,
    /// Names the selected faces as one surface group.
    CreateSurfaceGroup,
    /// Changes the visible name of the active group. The id stays.
    RenameSurfaceGroup,
    /// Puts the selected faces into the group named by the Groups list.
    AddSurfaceGroupFaces,
    /// Drops the selected faces from their group. Geometry stays.
    RemoveSurfaceGroupFaces,
    /// Replaces the face selection with the resolved faces of the named group.
    SelectSurfaceGroup,
    /// Removes the group record. Geometry and the painted slots stay.
    DeleteSurfaceGroup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlClass {
    Label,
    Edit,
    Check,
    Button,
    Combo,
}

/// How an inspector button is drawn. Native is a standard Win32 control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlFace {
    Native,
    /// One cell of the Select row, or Cancel and Done.
    Chip,
    /// Icon and a short name. Model tools use a two-column grid of these.
    Tile,
    /// Full-width dark row. Section titles and history actions use this.
    Row,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InspectorBinding {
    Label,
    Text { type_id: TypeId, field: FieldId },
    Axis { type_id: TypeId, field: FieldId, axis: u8 },
    Check { type_id: TypeId, field: FieldId },
    /// Editor cage toggle. Not a component field.
    ShowGrid,
    Choice { type_id: TypeId, field: FieldId },
    Command { command: InspectorCommand, type_id: TypeId },
    Section { title: String },
    AddComponent,
    Browse { type_id: TypeId, field: FieldId },
    Color { type_id: TypeId, field: FieldId },
    Uniform,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedControl {
    pub class: ControlClass,
    pub binding: InspectorBinding,
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub stretch: bool,
    pub enabled: bool,
    pub right_gutter: i32,
    pub right_slot: Option<i32>,
    pub choices: Vec<String>,
    pub selected: usize,
    pub checked: bool,
    pub face: ControlFace,
    /// More than one means this control shares its row. Zero is a normal stacked control.
    pub row_columns: u8,
    pub row_index: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanOptions {
    pub banner: Option<String>,
    pub advanced_open: bool,
    pub collapsed: Vec<String>,
    pub uniform_scale: bool,
    pub show_commands: bool,
    pub show_add: bool,
    /// View > Show Grid. The Geometry check reads this. It is not a saved field.
    pub show_grid: bool,
}

impl PlanOptions {
    pub fn editing() -> Self {
        Self {
            banner: None,
            advanced_open: false,
            collapsed: Vec::new(),
            uniform_scale: false,
            show_commands: true,
            show_add: true,
            show_grid: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct InspectorField {
    pub type_id: TypeId,
    pub field: FieldId,
    pub label: String,
    pub units: String,
    pub display: String,
    pub components: Vec<String>,
    pub editable: bool,
    pub kind: ValueKind,
    pub widget: WidgetKind,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub precision: u8,
    pub step: f64,
    pub choices: Vec<String>,
    pub asset_type: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InspectorSection {
    pub title: String,
    pub type_id: TypeId,
    pub fields: Vec<InspectorField>,
    pub commands: Vec<InspectorCommand>,
    /// Buttons whose label is not the static command name. The bool is the active mode chip.
    pub feature_edits: Vec<(InspectorCommand, String, bool)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InspectorBody {
    Empty,
    Multiple { count: usize, primary_name: String },
    Entity { sections: Vec<InspectorSection> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct InspectorModel {
    pub selection_revision: u64,
    pub world_revision: u64,
    pub header_name: String,
    pub header_kind: String,
    pub body: InspectorBody,
}

impl InspectorModel {
    pub fn empty() -> Self {
        Self { selection_revision: 0, world_revision: 0, header_name: String::new(), header_kind: String::new(), body: InspectorBody::Empty }
    }

    pub fn section_count(&self) -> usize {
        match &self.body {
            InspectorBody::Entity { sections } => sections.len(),
            _ => 0,
        }
    }

    pub fn field_count(&self) -> usize {
        match &self.body {
            InspectorBody::Entity { sections } => sections.iter().map(|section| section.fields.len()).sum(),
            _ => 0,
        }
    }

    pub fn field(&self, type_id: TypeId, field: FieldId) -> Option<&InspectorField> {
        let InspectorBody::Entity { sections } = &self.body else { return None };
        sections.iter().flat_map(|section| section.fields.iter()).find(|slot| slot.type_id == type_id && slot.field == field)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn section_titles(&self) -> Vec<&str> {
        match &self.body {
            InspectorBody::Entity { sections } => sections.iter().map(|section| section.title.as_str()).collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn build(selection: &SelectionService, world: &jarvig_core::SceneWorld) -> InspectorModel {
    build_with(selection, world, &[], &[])
}

/// `staged` is the loaded material catalog. Empty keeps a builtin material as read-only text.
pub fn build_with(selection: &SelectionService, world: &jarvig_core::SceneWorld, staged: &[String], mesh_assets: &[String]) -> InspectorModel {
    build_solid(selection, world, staged, mesh_assets, SolidElement::Object, "Auto")
}

/// Object tools or face tools, plus the active selection mode name drawn on the mode buttons.
pub fn build_solid(
    selection: &SelectionService,
    world: &jarvig_core::SceneWorld,
    staged: &[String],
    mesh_assets: &[String],
    element: SolidElement,
    mode_name: &str,
) -> InspectorModel {
    let selection_revision = selection.revision();
    let world_revision = world.revision();
    let body = match selection.items() {
        [] => InspectorBody::Empty,
        [SelectionItem::Entity(id)] => match world.inspect_entity(*id) {
            Ok(inspection) => InspectorBody::Entity { sections: sections_from(inspection, world, staged, mesh_assets, element, mode_name) },
            Err(_) => InspectorBody::Empty,
        },
        items => {
            let primary_name = selection.primary_entity().map(|id| entity_label(world, id)).unwrap_or_else(|| "Missing".into());
            InspectorBody::Multiple { count: items.len(), primary_name }
        }
    };
    let (header_name, header_kind) = match selection.items() {
        [SelectionItem::Entity(id)] => headers(world, *id),
        _ => (String::new(), String::new()),
    };
    InspectorModel { selection_revision, world_revision, header_name, header_kind, body }
}

fn entity_label(world: &jarvig_core::SceneWorld, id: EntityUuid) -> String {
    world.entity_outline().into_iter().find(|row| row.uuid == id).map(|row| {
        if row.name.is_empty() { "Unnamed".into() } else { row.name }
    }).unwrap_or_else(|| "Missing".into())
}

fn headers(world: &jarvig_core::SceneWorld, id: EntityUuid) -> (String, String) {
    let Some(row) = world.entity_outline().into_iter().find(|row| row.uuid == id) else {
        return ("Missing".into(), "Actor".into());
    };
    let name = if row.name.is_empty() { "Unnamed".into() } else { row.name };
    let kind = match row.class {
        AuthoringClass::Mesh => "Mesh Actor",
        AuthoringClass::DirectionalLight => "Directional Light",
        AuthoringClass::PointLight => "Point Light",
        AuthoringClass::SpotLight => "Spot Light",
        AuthoringClass::ReflectionProbe => "Reflection Probe",
        AuthoringClass::Camera => "Camera",
        AuthoringClass::WorldSettings => "World Settings",
        AuthoringClass::Terrain => "Terrain",
        AuthoringClass::PlayerStart => "Player Start",
        AuthoringClass::Block => "Block",
        AuthoringClass::Empty => "Actor",
    };
    (name, kind.into())
}

fn sections_from(
    inspection: EntityInspection,
    world: &jarvig_core::SceneWorld,
    staged: &[String],
    mesh_assets: &[String],
    element: SolidElement,
    mode_name: &str,
) -> Vec<InspectorSection> {
    let entity = inspection.entity;
    let present: Vec<TypeId> = inspection.sections.iter().map(|section| section.type_info.id).collect();
    let settings = world.entity_outline().iter().any(|row| row.uuid == entity && row.class == AuthoringClass::WorldSettings);
    let parents = parent_entries(world, entity);
    let mut sections = Vec::new();
    for section in inspection.sections {
        for field in section.fields {
            let title = panel_title(&field.info);
            let view = view_field(section.type_info.id, &field, staged, mesh_assets, settings, &parents);
            if let Some(existing) = sections.iter_mut().find(|panel: &&mut InspectorSection| panel.title == title) {
                existing.fields.push(view);
            } else {
                sections.push(InspectorSection { title, type_id: section.type_info.id, fields: vec![view], commands: Vec::new(), feature_edits: Vec::new() });
            }
        }
    }
    attach_commands(&mut sections, &present);
    hide_permanent_feature_fields(&mut sections);
    if let Ok(ownership) = world.entity_ownership(entity) {
        let record = world.authored_block(entity);
        attach_solid_tools(&mut sections, record.as_ref(), AuthoringCapabilities::from_entity(ownership.capabilities), element, mode_name);
    }
    sections
}

/// Bevel, the six insets, and the raw history log stay on the record. The panel shows current parameters instead.
fn hide_permanent_feature_fields(sections: &mut Vec<InspectorSection>) {
    for section in sections.iter_mut() {
        section.fields.retain(|field| {
            !matches!(
                field.field,
                jarvig_core::FIELD_BLOCK_BEVEL
                    | jarvig_core::FIELD_BLOCK_INSET_PX
                    | jarvig_core::FIELD_BLOCK_INSET_NX
                    | jarvig_core::FIELD_BLOCK_INSET_PY
                    | jarvig_core::FIELD_BLOCK_INSET_NY
                    | jarvig_core::FIELD_BLOCK_INSET_PZ
                    | jarvig_core::FIELD_BLOCK_INSET_NZ
                    | jarvig_core::FIELD_BLOCK_HISTORY
            )
        });
    }
    sections.retain(|section| !(section.title == "Modeling" && section.fields.is_empty() && section.commands.is_empty() && section.feature_edits.is_empty()));
}

/// Which part of a parametric solid the inspector is editing.
///
/// Face ids on a stored body are not analytic face indexes. Edge and vertex ids
/// are the solid's own ids. Zero means that mode is on and nothing is selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolidElement {
    Object,
    Face(u8),
    BodyFace(u32),
    Edge(u32),
    Vertex(u32),
}

fn attach_solid_tools(
    sections: &mut Vec<InspectorSection>,
    record: Option<&jarvig_core::BlockRecord>,
    capabilities: AuthoringCapabilities,
    element: SolidElement,
    mode_name: &str,
) {
    if !capabilities.parametric_solid {
        return;
    }
    let Some(record) = record else { return };
    if record.has_authored_seed() {
        for section in sections.iter_mut() {
            for field in &mut section.fields {
                if matches!(
                    field.field,
                    jarvig_core::FIELD_BLOCK_SIZE_X | jarvig_core::FIELD_BLOCK_SIZE_Y | jarvig_core::FIELD_BLOCK_SIZE_Z
                ) {
                    field.editable = false;
                }
            }
        }
    }
    let mode_edits = [
        ("Auto", InspectorCommand::SelectionAuto),
        ("Object", InspectorCommand::SelectionObject),
        ("Face", InspectorCommand::SelectionFace),
        ("Edge", InspectorCommand::SelectionEdge),
        ("Vert", InspectorCommand::SelectionVertex),
    ]
    .into_iter()
    .map(|(name, command)| {
        let active = name == mode_name || (name == "Vert" && mode_name == "Vertex");
        (command, name.to_string(), active)
    })
    .collect();
    sections.push(InspectorSection {
        title: "Select".into(),
        type_id: TYPE_PARAMETRIC_BLOCK,
        fields: Vec::new(),
        commands: vec![
            InspectorCommand::SelectLoop,
            InspectorCommand::SelectRing,
            InspectorCommand::SelectConnected,
            InspectorCommand::SelectBoundary,
            InspectorCommand::SelectGrow,
            InspectorCommand::SelectShrink,
        ],
        feature_edits: mode_edits,
    });
    let mut feature_edits = Vec::new();
    if record.bevel_m > 1.0e-9 {
        feature_edits.push((InspectorCommand::EditBevel, format!("Bevel  {:.3} m", record.bevel_m), false));
    }
    for face in 0..6u8 {
        let inset = record.inset_m[face as usize];
        if inset > 1.0e-9 {
            feature_edits.push((
                InspectorCommand::EditInset(face),
                format!("Inset {}  {inset:.3} m", jarvig_core::face_name(face)),
                false,
            ));
        }
    }
    sections.push(InspectorSection {
        title: "History".into(),
        type_id: TYPE_PARAMETRIC_BLOCK,
        fields: history_fields(record),
        commands: Vec::new(),
        feature_edits,
    });
    let face_tools = vec![
        InspectorCommand::ExtrudeFace,
        InspectorCommand::InsetFace,
        InspectorCommand::Bevel,
        InspectorCommand::SubdivideFace,
        InspectorCommand::SubdivideFace4,
    ];
    let edge_tools = vec![
        InspectorCommand::MoveEdge,
        InspectorCommand::ExtrudeEdge,
        InspectorCommand::SplitEdge,
        InspectorCommand::Bevel,
        InspectorCommand::Round,
    ];
    let mut object_tools = vec![InspectorCommand::Bevel, InspectorCommand::ResetShape];
    if capabilities.patternable {
        object_tools.extend([
            InspectorCommand::Duplicate,
            InspectorCommand::MirrorX,
            InspectorCommand::MirrorY,
            InspectorCommand::MirrorZ,
            InspectorCommand::AlignToGround,
            InspectorCommand::SnapTranslation,
        ]);
    }
    let commands = match mode_name {
        "Edge" => edge_tools,
        "Vertex" => vec![InspectorCommand::MoveVertex],
        "Face" => face_tools.clone(),
        "Object" => object_tools,
        _ => match element {
            SolidElement::Edge(id) if id != 0 => edge_tools,
            SolidElement::Vertex(id) if id != 0 => vec![InspectorCommand::MoveVertex],
            SolidElement::Face(_) | SolidElement::BodyFace(_) => face_tools,
            _ => object_tools,
        },
    };
    sections.push(InspectorSection {
        title: "Model".into(),
        type_id: TYPE_PARAMETRIC_BLOCK,
        fields: element_fields(record, element),
        commands,
        feature_edits: Vec::new(),
    });
    sections.push(InspectorSection {
        title: "Geometry".into(),
        type_id: TYPE_PARAMETRIC_BLOCK,
        fields: geometry_fields(record),
        commands: Vec::new(),
        feature_edits: Vec::new(),
    });
    sections.push(surface_section(record, &surface_faces(record, element)));
    order_solid_sections(sections);
}

fn surface_faces(record: &jarvig_core::BlockRecord, element: SolidElement) -> Vec<u32> {
    match element {
        SolidElement::BodyFace(id) if id != 0 => vec![id],
        SolidElement::Face(index) if presented_body(record).is_some() => vec![u32::from(index) + 1],
        _ => Vec::new(),
    }
}

/// Replaces the Surface card from the whole face selection. An empty list edits slot 0.
pub fn set_surface_material(model: &mut InspectorModel, record: &jarvig_core::BlockRecord, faces: &[u32]) {
    let InspectorBody::Entity { sections } = &mut model.body else { return };
    sections.retain(|section| section.title != "Surface");
    sections.push(surface_section(record, faces));
    order_solid_sections(sections);
}

/// True when every live face on `slot` is in `faces`. An empty face list is the object card.
pub fn selection_owns_material(record: &jarvig_core::BlockRecord, faces: &[u32], slot: u32) -> bool {
    if faces.is_empty() {
        return true;
    }
    let owners = record.faces_on_slot(slot);
    !owners.is_empty() && owners.iter().all(|face| faces.contains(face))
}

/// Short name of a block material slot. Slot 0 is the object material.
pub fn surface_slot_name(slot: u32) -> String {
    if slot == 0 {
        "Object Material".into()
    } else {
        format!("Face Material {slot}")
    }
}

/// Inspector title, including the slot index.
pub fn surface_slot_title(slot: u32) -> String {
    format!("{} (Slot {slot})", surface_slot_name(slot))
}

fn surface_section(record: &jarvig_core::BlockRecord, faces: &[u32]) -> InspectorSection {
    let mut used = Vec::new();
    for face in faces {
        let slot = record.bound_slot(*face);
        if !used.contains(&slot) {
            used.push(slot);
        }
    }
    let shared = if faces.is_empty() {
        Some(0)
    } else if used.len() == 1 {
        Some(used[0])
    } else {
        None
    };
    let note = if faces.is_empty() {
        SURFACE_OBJECT_NOTE
    } else if shared.is_none() {
        SURFACE_MIXED_NOTE
    } else if shared == Some(0) && selection_owns_material(record, faces, 0) {
        SURFACE_WHOLE_NOTE
    } else if shared.is_some_and(|slot| selection_owns_material(record, faces, slot)) {
        SURFACE_OWN_NOTE
    } else {
        SURFACE_SHARED_NOTE
    };
    let mut fields = Vec::new();
    if !faces.is_empty() {
        let word = if faces.len() == 1 { "Face" } else { "Faces" };
        fields.push(solid_note(SOLID_UI_SURFACE_COUNT, "Selection", format!("{} {word}", faces.len())));
    }
    fields.push(solid_note(
        SOLID_UI_SURFACE_SLOT,
        "Material",
        shared.map(surface_slot_title).unwrap_or_else(|| "Mixed".into()),
    ));
    fields.push(solid_note(SOLID_UI_SURFACE_NOTE, "Notice", note.into()));
    if let jarvig_core::SurfaceGroupCover::Covered(id) = record.surface_group_cover(faces) {
        if let Some(slot) = shared {
            if record.surface_group(id).is_some_and(|group| group.slot == slot) {
                let resolved = record.resolved_group_faces(id);
                if selection_owns_material(record, &resolved, slot) && !selection_owns_material(record, faces, slot) {
                    fields.last_mut().map(|field| field.display = SURFACE_GROUP_OWN_NOTE.into());
                } else if !selection_owns_material(record, &resolved, slot) {
                    fields.last_mut().map(|field| field.display = SURFACE_GROUP_SHARED_NOTE.into());
                }
            }
        }
    }
    if !faces.is_empty() {
        let mut choices: Vec<String> = (0..record.slot_count()).map(surface_slot_name).collect();
        let display = if let Some(slot) = shared { surface_slot_name(slot) } else { "Mixed".into() };
        if shared.is_none() {
            choices.insert(0, "Mixed".into());
        }
        fields.push(surface_choice(SOLID_UI_SURFACE_ASSIGN, "Assign Existing", &display, choices));
    }
    if let Some(slot) = shared {
        if let Some(material) = record.material_slot(slot) {
            fields.push(factor_field(SOLID_UI_COLOR_R, "R", material.base_color[0]));
            fields.push(factor_field(SOLID_UI_COLOR_G, "G", material.base_color[1]));
            fields.push(factor_field(SOLID_UI_COLOR_B, "B", material.base_color[2]));
            fields.push(factor_field(SOLID_UI_ROUGHNESS, "Roughness", material.roughness));
            fields.push(factor_field(SOLID_UI_METALLIC, "Metallic", material.metallic));
        }
    }
    let mut commands = Vec::new();
    if faces.is_empty() {
        commands.push(InspectorCommand::SelectSlotFaces);
        commands.push(InspectorCommand::AddMaterialSlot);
    } else {
        commands.push(InspectorCommand::MakeUnique);
        if shared.is_some() {
            commands.push(InspectorCommand::SelectSlotFaces);
        }
    }
    let (group_display, group_choices) = group_pick(record, faces);
    fields.push(solid_note(SOLID_UI_GROUP_MEMBERSHIP, "Group", group_membership_label(record, faces)));
    fields.push(surface_name(SOLID_UI_GROUP_NAME, "Group Name", &group_name_display(record, faces)));
    fields.push(surface_choice(SOLID_UI_GROUP_PICK, "Groups", &group_display, group_choices));
    commands.push(InspectorCommand::CreateSurfaceGroup);
    commands.push(InspectorCommand::RenameSurfaceGroup);
    commands.push(InspectorCommand::AddSurfaceGroupFaces);
    commands.push(InspectorCommand::RemoveSurfaceGroupFaces);
    commands.push(InspectorCommand::SelectSurfaceGroup);
    commands.push(InspectorCommand::DeleteSurfaceGroup);
    InspectorSection {
        title: "Surface".into(),
        type_id: TYPE_PARAMETRIC_BLOCK,
        fields,
        commands,
        feature_edits: Vec::new(),
    }
}

fn group_membership_label(record: &jarvig_core::BlockRecord, faces: &[u32]) -> String {
    if faces.is_empty() {
        return "None".into();
    }
    match record.surface_group_cover(faces) {
        jarvig_core::SurfaceGroupCover::None => "None".into(),
        jarvig_core::SurfaceGroupCover::Mixed => "Mixed".into(),
        jarvig_core::SurfaceGroupCover::Covered(id) => {
            let Some(group) = record.surface_group(id) else { return "None".into() };
            if group.members.iter().any(|member| member.face.is_none()) {
                format!("{} (unresolved)", group.name)
            } else {
                group.name.clone()
            }
        }
    }
}

fn group_name_display(record: &jarvig_core::BlockRecord, faces: &[u32]) -> String {
    match record.surface_group_cover(faces) {
        jarvig_core::SurfaceGroupCover::Covered(id) => record.surface_group(id).map(|group| group.name.clone()).unwrap_or_default(),
        _ => String::new(),
    }
}

fn group_pick(record: &jarvig_core::BlockRecord, faces: &[u32]) -> (String, Vec<String>) {
    let mut choices: Vec<String> = record.surface_groups.iter().map(|group| group.name.clone()).collect();
    if choices.is_empty() {
        return ("None".into(), vec!["None".into()]);
    }
    if let jarvig_core::SurfaceGroupCover::Covered(id) = record.surface_group_cover(faces) {
        if let Some(group) = record.surface_group(id) {
            return (group.name.clone(), choices);
        }
    }
    if choices.len() == 1 {
        let name = choices[0].clone();
        return (name, choices);
    }
    choices.insert(0, "None".into());
    ("None".into(), choices)
}

fn surface_name(field: FieldId, label: &str, display: &str) -> InspectorField {
    let mut field = solid_note(field, label, display.into());
    field.kind = ValueKind::String;
    field.widget = WidgetKind::Text;
    field.editable = true;
    field
}

fn surface_choice(field: FieldId, label: &str, selected: &str, choices: Vec<String>) -> InspectorField {
    InspectorField {
        type_id: TYPE_PARAMETRIC_BLOCK,
        field,
        label: label.into(),
        units: String::new(),
        display: selected.into(),
        components: Vec::new(),
        editable: true,
        kind: ValueKind::String,
        widget: WidgetKind::Choice,
        minimum: None,
        maximum: None,
        precision: 0,
        step: 0.0,
        choices,
        asset_type: String::new(),
    }
}

fn factor_field(field: FieldId, label: &str, value: f32) -> InspectorField {
    let mut field = solid_note(field, label, format_fixed(f64::from(value), 3));
    field.kind = ValueKind::Float64;
    field.widget = WidgetKind::Number;
    field.editable = true;
    field.minimum = Some(0.0);
    field.maximum = Some(1.0);
    field.precision = 3;
    field.step = 0.01;
    field
}

/// Counts and measures for the inspector. An authored seed replays once here.
/// Hover and the frame cache do not.
fn presented_body(record: &jarvig_core::BlockRecord) -> Option<jarvig_core::SolidBody> {
    if record.has_authored_seed() && record.body.is_none() {
        if let jarvig_core::IntentAuthorityCandidate::Reconstructable(body) = jarvig_core::intent_authority_candidate(record) {
            return Some(body);
        }
    }
    record.display_body()
}

fn element_fields(record: &jarvig_core::BlockRecord, element: SolidElement) -> Vec<InspectorField> {
    let body = presented_body(record);
    match element {
        SolidElement::Object => Vec::new(),
        SolidElement::Face(face) => {
            let mut fields = vec![solid_note(SOLID_UI_FACE, "Face", jarvig_core::face_name(face).to_string())];
            fields.push(solid_note(SOLID_UI_AREA, "Area", format!("{:.3} m²", analytic_face_area(record.size_m, face))));
            fields.push(solid_note(SOLID_UI_NORMAL, "Normal", jarvig_core::face_name(face).to_string()));
            fields
        }
        SolidElement::BodyFace(0) => vec![solid_note(SOLID_UI_FACE, "Face", "None".into())],
        SolidElement::BodyFace(face) => {
            let mut fields = vec![solid_note(SOLID_UI_FACE, "Face", face_identity_label(record, face))];
            if let Some(body) = &body {
                if let Some(area) = body.face_area(face) {
                    fields.push(solid_note(SOLID_UI_AREA, "Area", format!("{area:.3} m²")));
                }
                if let Some(normal) = body.unit_normal(face) {
                    fields.push(solid_note(SOLID_UI_NORMAL, "Normal", normal_label(normal)));
                }
            }
            fields
        }
        SolidElement::Edge(0) => vec![solid_note(SOLID_UI_FACE, "Edge", "None".into())],
        SolidElement::Edge(edge) => {
            let mut fields = vec![solid_note(SOLID_UI_FACE, "Edge", edge_identity_label(record, edge))];
            if let Some(length) = body.as_ref().and_then(|body| body.edge_length(edge)) {
                fields.push(solid_note(SOLID_UI_LENGTH, "Length", format!("{length:.3} m")));
            }
            fields
        }
        SolidElement::Vertex(0) => vec![solid_note(SOLID_UI_FACE, "Vertex", "None".into())],
        SolidElement::Vertex(vertex) => vec![solid_note(SOLID_UI_FACE, "Vertex", vertex_identity_label(record, vertex))],
    }
}

fn geometry_fields(record: &jarvig_core::BlockRecord) -> Vec<InspectorField> {
    let (faces, edges, vertices) = if let Some(body) = presented_body(record) {
        (body.faces.len().to_string(), body.edges.len().to_string(), body.vertices.len().to_string())
    } else {
        ("—".into(), "—".into(), "—".into())
    };
    let mut fields = vec![
        solid_note(SOLID_UI_FACES, "Faces", faces),
        solid_note(SOLID_UI_EDGES, "Edges", edges),
        solid_note(SOLID_UI_VERTS, "Vertices", vertices),
        solid_note(SOLID_UI_CLOSED, "Closed", "Yes".into()),
    ];
    if record.body.is_some() {
        let mut grid = solid_note(SOLID_UI_GRID, "Show Grid", String::new());
        grid.widget = WidgetKind::Check;
        grid.editable = true;
        fields.push(grid);
    }
    fields
}

fn history_fields(record: &jarvig_core::BlockRecord) -> Vec<InspectorField> {
    let mut fields = vec![solid_note(
        SOLID_UI_BLOCK,
        "Block",
        format!("{:.3} × {:.3} × {:.3} m", record.size_m[0], record.size_m[1], record.size_m[2]),
    )];
    let start = record.history.len().saturating_sub(8);
    let shown = &record.history[start..];
    for (offset, op) in shown.iter().enumerate().rev() {
        let number = start + offset + 1;
        fields.push(solid_note(FieldId(260 + number as u32), &format!("{number:02}"), op.summary()));
    }
    fields
}

/// The inspector name of one face. An authored seed uses the replayed token.
/// A stored body keeps the concrete id. A triangle index is not a name.
pub(crate) fn face_identity_label(record: &jarvig_core::BlockRecord, face: u32) -> String {
    if record.has_authored_seed() && record.body.is_none() {
        if let Some(name) = jarvig_core::preferred_face_token(&jarvig_core::semantic_face_names(record, face)) {
            return name;
        }
    }
    format!("F:{face}")
}

/// The inspector name of one edge. An authored seed uses the persistent token.
/// A stored body keeps the concrete id.
pub(crate) fn edge_identity_label(record: &jarvig_core::BlockRecord, edge: u32) -> String {
    if record.has_authored_seed() && record.body.is_none() {
        if let Some(name) = jarvig_core::semantic_edge_names(record, edge).into_iter().find(|name| name.starts_with("E:fillet-")) {
            return name;
        }
        if let jarvig_core::IntentAuthorityCandidate::Reconstructable(body) = jarvig_core::intent_authority_candidate(record) {
            if let Ok(bindings) = jarvig_core::semantic_edge_bindings(record) {
                if let Ok(token) = jarvig_core::persistent_edge_token(&body, &bindings, edge) {
                    return token;
                }
            }
        }
    }
    format!("E:{edge}")
}

/// The inspector name of one vertex. An authored seed uses the replayed token.
pub(crate) fn vertex_identity_label(record: &jarvig_core::BlockRecord, vertex: u32) -> String {
    if record.has_authored_seed() && record.body.is_none() {
        if let Some(name) = jarvig_core::semantic_vertex_names(record, vertex).into_iter().next() {
            return name;
        }
    }
    format!("V:{vertex}")
}

fn analytic_face_area(size: [f64; 3], face: u8) -> f64 {
    match face {
        0 | 1 => size[1] * size[2],
        2 | 3 => size[0] * size[2],
        _ => size[0] * size[1],
    }
}

fn normal_label(normal: [f64; 3]) -> String {
    let axes = ["X", "Y", "Z"];
    let mut axis = 0usize;
    for index in 1..3 {
        if normal[index].abs() > normal[axis].abs() {
            axis = index;
        }
    }
    if normal[axis].abs() >= 0.999 {
        format!("{}{}", if normal[axis] >= 0.0 { "+" } else { "-" }, axes[axis])
    } else {
        format!("{:.2}, {:.2}, {:.2}", normal[0], normal[1], normal[2])
    }
}

fn order_solid_sections(sections: &mut Vec<InspectorSection>) {
    let rank = |title: &str| match title {
        "Actor" => 0,
        "Select" => 1,
        "Transform" => 2,
        "Dimensions" => 3,
        "Model" => 4,
        "Active Tool" => 5,
        "Geometry" => 6,
        "History" => 7,
        "Collision" => 8,
        "Material" => 9,
        "Surface" => 10,
        "Advanced" => 30,
        "Components" => 31,
        _ => 20,
    };
    sections.sort_by_key(|section| rank(section.title.as_str()));
}

/// The open modeling operation. A drag commits on mouse-up. Amount stays for a typed value.
pub struct ModelingView {
    pub title: &'static str,
    pub face: String,
    pub amount_label: &'static str,
    pub amount: f64,
    pub minimum: f64,
    pub maximum: f64,
    /// False for Move Edge, Extrude Edge, and Move Vertex. Those drags have no single distance.
    pub show_amount: bool,
    pub element_label: &'static str,
    /// Round shows Circular. Other tools leave this empty.
    pub profile: Option<&'static str>,
    /// Round shows the observation note. Other tools leave this empty.
    pub resolution: Option<&'static str>,
    /// Round shows the radii the edge can close. Other tools leave this empty.
    pub range: Option<String>,
    /// How many persistent semantic edges the picked topology resolved to.
    pub resolved: Option<String>,
    /// Valid, Clamped, or Conflict.
    pub readiness: Option<String>,
    /// Why several picked edges became one semantic edge, or why Apply will refuse.
    pub note: Option<String>,
}

/// Hides the tool shelf and opens the Active Tool card. Mode chips and history stay.
pub fn attach_modeling_session(model: &mut InspectorModel, view: &ModelingView) {
    let InspectorBody::Entity { sections } = &mut model.body else { return };
    for section in sections.iter_mut() {
        section.commands.retain(|command| !modeling_start(*command));
        section.feature_edits.retain(|(command, _, _)| mode_chip(*command) || feature_reopen(*command));
        section.fields.retain(|field| !matches!(field.field, SOLID_UI_FACE | SOLID_UI_AREA | SOLID_UI_NORMAL | SOLID_UI_LENGTH));
    }
    sections.retain(|section| section.title != "Surface");
    sections.retain(|section| section.title != "Model" || !section.fields.is_empty() || !section.commands.is_empty());
    let mut fields = vec![
        solid_note(SOLID_UI_TOOL, "Tool", view.title.into()),
        solid_note(SOLID_UI_FACE, view.element_label, view.face.clone()),
    ];
    if view.show_amount {
        fields.push(solid_amount(view));
    }
    if let Some(profile) = view.profile {
        fields.push(solid_note(SOLID_UI_PROFILE, "Profile", profile.into()));
    }
    if let Some(resolution) = view.resolution {
        fields.push(solid_note(SOLID_UI_RESOLUTION, "Render Resolution", resolution.into()));
    }
    if let Some(range) = &view.range {
        fields.push(solid_note(SOLID_UI_RANGE, "Valid Range", range.clone()));
    }
    if let Some(resolved) = &view.resolved {
        fields.push(solid_note(SOLID_UI_RESOLVED, "Resolved semantic edges", resolved.clone()));
    }
    if let Some(readiness) = &view.readiness {
        fields.push(solid_note(SOLID_UI_STATUS, "Status", readiness.clone()));
    }
    if let Some(note) = &view.note {
        fields.push(solid_note(SOLID_UI_ROUND_NOTE, "Note", note.clone()));
    }
    let commands = if view.title == "Round" {
        vec![InspectorCommand::CancelModeling, InspectorCommand::ApplyRound]
    } else {
        vec![InspectorCommand::CancelModeling, InspectorCommand::ApplyModeling]
    };
    sections.retain(|section| section.title != "Active Tool");
    sections.push(InspectorSection {
        title: "Active Tool".into(),
        type_id: TYPE_PARAMETRIC_BLOCK,
        fields,
        commands,
        feature_edits: Vec::new(),
    });
    order_solid_sections(sections);
}

fn modeling_start(command: InspectorCommand) -> bool {
    matches!(
        command,
        InspectorCommand::ExtrudeFace
            | InspectorCommand::InsetFace
            | InspectorCommand::Bevel
            | InspectorCommand::Round
            | InspectorCommand::ResetShape
            | InspectorCommand::MoveEdge
            | InspectorCommand::ExtrudeEdge
            | InspectorCommand::SplitEdge
            | InspectorCommand::MoveVertex
            | InspectorCommand::SubdivideFace
            | InspectorCommand::SubdivideFace4
    )
}

fn mode_chip(command: InspectorCommand) -> bool {
    matches!(
        command,
        InspectorCommand::SelectionAuto
            | InspectorCommand::SelectionObject
            | InspectorCommand::SelectionFace
            | InspectorCommand::SelectionEdge
            | InspectorCommand::SelectionVertex
    )
}

fn feature_reopen(command: InspectorCommand) -> bool {
    matches!(command, InspectorCommand::EditBevel | InspectorCommand::EditInset(_))
}

fn solid_amount(view: &ModelingView) -> InspectorField {
    let mut field = solid_note(SOLID_UI_AMOUNT, view.amount_label, format_fixed(view.amount, 3));
    field.kind = ValueKind::Float64;
    field.widget = WidgetKind::Number;
    field.editable = true;
    field.units = "m".into();
    field.minimum = Some(view.minimum);
    field.maximum = Some(view.maximum);
    field.precision = 3;
    field.step = 0.001;
    field
}

fn solid_note(field: FieldId, label: &str, display: String) -> InspectorField {
    InspectorField {
        type_id: TYPE_PARAMETRIC_BLOCK,
        field,
        label: label.into(),
        units: String::new(),
        display,
        components: Vec::new(),
        editable: false,
        kind: ValueKind::String,
        widget: WidgetKind::Readonly,
        minimum: None,
        maximum: None,
        precision: 0,
        step: 0.0,
        choices: Vec::new(),
        asset_type: String::new(),
    }
}

/// Drops the single-element area, normal, and length when several elements are selected.
pub fn hide_single_element_measures(model: &mut InspectorModel) {
    let InspectorBody::Entity { sections } = &mut model.body else { return };
    for section in sections.iter_mut() {
        section.fields.retain(|field| !matches!(field.field, SOLID_UI_AREA | SOLID_UI_NORMAL | SOLID_UI_LENGTH));
    }
}

/// The viewport's selected face. Display only. It is not a saved field.
pub fn set_selected_face_label(model: &mut InspectorModel, label: &str) {
    let InspectorBody::Entity { sections } = &mut model.body else { return };
    for section in sections.iter_mut() {
        if let Some(field) = section.fields.iter_mut().find(|field| field.field == SOLID_UI_FACE) {
            field.display = label.to_string();
        }
    }
}

fn panel_title(info: &jarvig_core::FieldInfo) -> String {
    if info.id == FIELD_UUID {
        "Advanced".into()
    } else if info.id == FIELD_NAME || info.id == FIELD_PARENT {
        "Actor".into()
    } else if info.group == "Shape" {
        "Dimensions".into()
    } else {
        info.group.to_string()
    }
}

fn view_field(
    type_id: TypeId,
    field: &jarvig_core::InspectedField,
    staged: &[String],
    mesh_assets: &[String],
    settings: bool,
    parents: &[(String, Option<EntityUuid>)],
) -> InspectorField {
    let info = &field.info;
    let mut editable = info.editable;
    let mut choices: Vec<String> = info.enum_values.iter().map(|value| (*value).to_string()).collect();
    if info.id == FIELD_SURFACE {
        let primitive = matches!(field.display.as_str(), "Cube" | "Sphere" | "Plane");
        let imported = mesh_assets.iter().any(|name| name == &field.display);
        if primitive || imported {
            editable = true;
            choices = vec!["Cube".into(), "Sphere".into(), "Plane".into()];
            for name in mesh_assets {
                if !choices.iter().any(|choice| choice == name) {
                    choices.push(name.clone());
                }
            }
        } else {
            editable = false;
            choices.clear();
        }
    }
    if info.id == FIELD_MATERIAL_SLOT {
        if staged.is_empty() {
            editable = false;
            choices.clear();
        } else {
            editable = true;
            choices = staged.to_vec();
            if !choices.iter().any(|choice| choice == &field.display) {
                choices.insert(0, field.display.clone());
            }
        }
    }
    if info.id == FIELD_PARENT {
        if settings {
            editable = false;
            choices.clear();
        } else {
            editable = true;
            choices = parents.iter().map(|(label, _)| label.clone()).collect();
        }
    }
    let mut widget = widget_for(info, editable, &choices);
    if !editable && matches!(widget, WidgetKind::Text | WidgetKind::Choice) && choices.is_empty() {
        widget = WidgetKind::Readonly;
    }
    let components = match &field.value {
        PropertyValue::Vec3(value) => vec![format_fixed(value.x, info.precision), format_fixed(value.y, info.precision), format_fixed(value.z, info.precision)],
        PropertyValue::Quat(value) => {
            let euler = value.euler_xyz_degrees();
            vec![format_fixed(euler.x, info.precision), format_fixed(euler.y, info.precision), format_fixed(euler.z, info.precision)]
        }
        _ => Vec::new(),
    };
    InspectorField {
        type_id,
        field: info.id,
        label: info.display_name.to_string(),
        units: info.units.to_string(),
        display: field.display.clone(),
        components,
        editable,
        kind: info.kind,
        widget,
        minimum: info.minimum,
        maximum: info.maximum,
        precision: info.precision,
        step: info.step,
        choices,
        asset_type: info.asset_type.to_string(),
    }
}

fn widget_for(info: &jarvig_core::FieldInfo, editable: bool, choices: &[String]) -> WidgetKind {
    if !choices.is_empty() {
        return WidgetKind::Choice;
    }
    match info.kind {
        ValueKind::Bool => WidgetKind::Check,
        ValueKind::Float64 => {
            if editable {
                WidgetKind::Number
            } else {
                WidgetKind::Readonly
            }
        }
        ValueKind::Vec3F64 if info.units == "linear" => WidgetKind::Color,
        ValueKind::Vec3F64 => WidgetKind::Vector,
        ValueKind::QuatF64 if info.hint == "euler-degrees" => WidgetKind::Euler,
        ValueKind::String if editable => WidgetKind::Text,
        _ => WidgetKind::Readonly,
    }
}

fn attach_commands(sections: &mut [InspectorSection], present: &[TypeId]) {
    let mut removed = Vec::new();
    for section in sections.iter_mut() {
        match section.title.as_str() {
            "Transform" => section.commands.push(InspectorCommand::ResetTransform),
            "Material Instance" => section.commands.push(InspectorCommand::ResetMaterial),
            "Camera" => section.commands.push(InspectorCommand::SetStartupCamera),
            "Reflection Probe" => section.commands.push(InspectorCommand::Recapture),
            _ => {}
        }
        if matches!(section.title.as_str(), "Actor" | "Advanced" | "Components" | "Materials" | "Material Instance") {
            continue;
        }
        if removed.contains(&section.type_id) || !can_remove(section.type_id, present) {
            continue;
        }
        section.commands.push(InspectorCommand::RemoveComponent);
        removed.push(section.type_id);
    }
}

fn can_remove(type_id: TypeId, present: &[TypeId]) -> bool {
    let Some(info) = find_type(type_id) else { return false };
    if matches!(info.multiplicity, ComponentMultiplicity::NotAComponent) || type_id == TYPE_ENVIRONMENT || type_id == TYPE_TERRAIN || type_id == TYPE_PARAMETRIC_BLOCK {
        return false;
    }
    !present.iter().any(|other| *other != type_id && find_type(*other).is_some_and(|existing| existing.requires.contains(&type_id)))
}

fn parent_entries(world: &jarvig_core::SceneWorld, entity: EntityUuid) -> Vec<(String, Option<EntityUuid>)> {
    let mut entries = vec![("World".to_string(), None)];
    let mut used = vec!["World".to_string()];
    for row in world.entity_outline() {
        if row.uuid == entity || row.class == AuthoringClass::WorldSettings {
            continue;
        }
        let mut label = if row.name.is_empty() { "Unnamed".to_string() } else { row.name };
        if used.iter().any(|existing| existing == &label) {
            let id = row.uuid.to_string();
            let short = id.get(..8).unwrap_or(&id);
            label = format!("{label} ({short})");
        }
        used.push(label.clone());
        entries.push((label, Some(row.uuid)));
    }
    entries
}

pub fn parent_from_choice(world: &jarvig_core::SceneWorld, entity: EntityUuid, choice: &str) -> Result<Option<EntityUuid>, &'static str> {
    parent_entries(world, entity)
        .into_iter()
        .find(|(label, _)| label == choice)
        .map(|(_, id)| id)
        .ok_or("that parent is not in the list")
}

pub fn choice_as_number(choice: &str) -> Option<f64> {
    if choice == "Default" {
        Some(0.0)
    } else {
        choice.parse::<f64>().ok().filter(|value| value.is_finite())
    }
}

pub fn clamp_number(value: f64, minimum: Option<f64>, maximum: Option<f64>) -> Option<f64> {
    if !value.is_finite() {
        return None;
    }
    let mut value = value;
    if let Some(minimum) = minimum {
        value = value.max(minimum);
    }
    if let Some(maximum) = maximum {
        value = value.min(maximum);
    }
    Some(value)
}

/// Components the registry will accept on this actor. Mesh Renderer is created with a mesh.
pub fn add_component_choices(owned: &[TypeId]) -> Vec<(TypeId, &'static str)> {
    let mut choices = Vec::new();
    for info in type_registry() {
        if matches!(info.multiplicity, ComponentMultiplicity::NotAComponent) {
            continue;
        }
        if matches!(info.id, TYPE_ENVIRONMENT | TYPE_MESH_RENDERER | TYPE_TERRAIN | TYPE_PARAMETRIC_BLOCK | TYPE_PAWN | TYPE_FREE_FLY | TYPE_ENTITY | TYPE_COMPONENT_STACK) {
            continue;
        }
        if matches!(info.multiplicity, ComponentMultiplicity::One) && owned.contains(&info.id) {
            continue;
        }
        if info.rejects.iter().any(|rejected| owned.contains(rejected)) {
            continue;
        }
        if owned.iter().any(|have| find_type(*have).is_some_and(|existing| existing.rejects.contains(&info.id))) {
            continue;
        }
        choices.push((info.id, info.display_name));
    }
    choices
}

pub fn owned_types(world: &jarvig_core::SceneWorld, entity: EntityUuid) -> Vec<TypeId> {
    let Ok(stack) = world.component_stack(entity) else { return Vec::new() };
    stack.iter().filter_map(|binding| type_for_role(binding.role)).collect()
}

fn type_for_role(role: ComponentRole) -> Option<TypeId> {
    Some(match role {
        ComponentRole::Transform => TYPE_SPATIAL_FRAME,
        ComponentRole::MeshRenderer => TYPE_MESH_RENDERER,
        ComponentRole::DirectionalLight => TYPE_DIRECTIONAL_LIGHT,
        ComponentRole::PointLight => TYPE_POINT_LIGHT,
        ComponentRole::SpotLight => TYPE_SPOT_LIGHT,
        ComponentRole::ReflectionProbe => TYPE_REFLECTION_PROBE,
        ComponentRole::Camera => jarvig_core::TYPE_CAMERA,
        ComponentRole::Pawn => TYPE_PAWN,
        ComponentRole::FreeFly => TYPE_FREE_FLY,
        ComponentRole::Joint => jarvig_core::TYPE_JOINT,
        ComponentRole::Terrain => jarvig_core::TYPE_TERRAIN,
        ComponentRole::PlayerStart => jarvig_core::TYPE_PLAYER_START,
        ComponentRole::Block => TYPE_PARAMETRIC_BLOCK,
        ComponentRole::WorldSettings => TYPE_ENVIRONMENT,
    })
}

/// Widget identity and placement. Text, the check, and the combo selection are values.
pub fn same_control_layout(left: &PlannedControl, right: &PlannedControl) -> bool {
    left.class == right.class
        && left.binding == right.binding
        && left.enabled == right.enabled
        && left.choices == right.choices
        && left.x == right.x
        && left.y == right.y
        && left.width == right.width
        && left.height == right.height
        && left.stretch == right.stretch
        && left.right_gutter == right.right_gutter
        && left.right_slot == right.right_slot
        && left.face == right.face
        && left.row_columns == right.row_columns
        && left.row_index == right.row_index
}

pub fn plan(model: &InspectorModel, options: &PlanOptions) -> Vec<PlannedControl> {
    let mut y = 8i32;
    let mut items = Vec::new();
    if let Some(banner) = &options.banner {
        items.push(control(ControlClass::Label, InspectorBinding::Label, banner.clone(), 12, y));
        y += 24;
    }
    match &model.body {
        InspectorBody::Empty => {
            items.push(control(ControlClass::Label, InspectorBinding::Label, "No selection.".into(), 12, y));
            y += 24;
            items.push(control(ControlClass::Label, InspectorBinding::Label, "Select one entity to edit it.".into(), 12, y));
        }
        InspectorBody::Multiple { count, primary_name } => {
            items.push(control(ControlClass::Label, InspectorBinding::Label, format!("{count} entities selected"), 12, y));
            y += 24;
            items.push(control(ControlClass::Label, InspectorBinding::Label, format!("Primary: {primary_name}"), 12, y));
            y += 24;
            items.push(control(ControlClass::Label, InspectorBinding::Label, "Multi-object editing is not implemented.".into(), 12, y));
        }
        InspectorBody::Entity { sections } => {
            if !model.header_name.is_empty() {
                items.push(control(ControlClass::Label, InspectorBinding::Label, model.header_name.to_uppercase(), 12, y));
                y += 22;
                items.push(control(ControlClass::Label, InspectorBinding::Label, model.header_kind.clone(), 12, y));
                y += 26;
            }
            for section in sections {
                if section.title == "Components" {
                    continue;
                }
                if section.title != "Actor" {
                    let open = if section.title == "Advanced" {
                        options.advanced_open
                    } else {
                        !options.collapsed.iter().any(|title| title == &section.title)
                    };
                    let marker = if open { "▼" } else { "▶" };
                    let header_commands: Vec<_> = section
                        .commands
                        .iter()
                        .copied()
                        .filter(|command| options.show_commands && command_in_header(*command))
                        .collect();
                    let mut header = control(
                        ControlClass::Button,
                        InspectorBinding::Section { title: section.title.clone() },
                        format!("{marker}  {}", section.title),
                        4,
                        y,
                    );
                    header.face = ControlFace::Row;
                    header.height = 22;
                    header.right_gutter = header_commands.len() as i32 * 76;
                    items.push(header);
                    for (index, command) in header_commands.iter().enumerate() {
                        let mut button = control(
                            ControlClass::Button,
                            InspectorBinding::Command { command: *command, type_id: section.type_id },
                            command_label(*command).into(),
                            0,
                            y,
                        );
                        button.stretch = false;
                        button.width = 72;
                        button.right_slot = Some((header_commands.len() - 1 - index) as i32);
                        items.push(button);
                    }
                    y += 24;
                    if !open {
                        continue;
                    }
                }
                if section.title == "Surface" {
                    y = push_surface(&mut items, section, y, options);
                    y += 8;
                    continue;
                }
                for field in &section.fields {
                    if field.field == FIELD_CAPTURE_STATE {
                        continue;
                    }
                    y = push_field(&mut items, field, y, options);
                }
                if options.show_commands {
                    push_shelf(&mut items, section, &mut y);
                }
                y += 8;
            }
            if options.show_add {
                y += 4;
                items.push(control(ControlClass::Button, InspectorBinding::AddComponent, "+ Add Component".into(), 12, y));
            }
        }
    }
    items
}

fn surface_factor(field: FieldId) -> bool {
    matches!(field, SOLID_UI_COLOR_R | SOLID_UI_COLOR_G | SOLID_UI_COLOR_B | SOLID_UI_ROUGHNESS | SOLID_UI_METALLIC)
}

fn surface_group_field(field: FieldId) -> bool {
    matches!(field, SOLID_UI_GROUP_MEMBERSHIP | SOLID_UI_GROUP_NAME | SOLID_UI_GROUP_PICK)
}

fn surface_group_command(command: InspectorCommand) -> bool {
    matches!(
        command,
        InspectorCommand::CreateSurfaceGroup
            | InspectorCommand::RenameSurfaceGroup
            | InspectorCommand::AddSurfaceGroupFaces
            | InspectorCommand::RemoveSurfaceGroupFaces
            | InspectorCommand::SelectSurfaceGroup
            | InspectorCommand::DeleteSurfaceGroup
    )
}

fn push_surface(items: &mut Vec<PlannedControl>, section: &InspectorSection, mut y: i32, options: &PlanOptions) -> i32 {
    for field in &section.fields {
        if surface_factor(field.field) || field.field == SOLID_UI_SURFACE_ASSIGN || surface_group_field(field.field) {
            continue;
        }
        if field.field == SOLID_UI_SURFACE_NOTE {
            let mut line = control(ControlClass::Label, InspectorBinding::Label, field.display.clone(), 12, y);
            line.height = 48;
            items.push(line);
            y += 52;
            continue;
        }
        y = push_field(items, field, y, options);
    }
    if options.show_commands {
        for command in &section.commands {
            if *command != InspectorCommand::MakeUnique {
                continue;
            }
            let mut button = control(
                ControlClass::Button,
                InspectorBinding::Command { command: *command, type_id: section.type_id },
                command_label(*command).into(),
                12,
                y,
            );
            button.face = ControlFace::Row;
            button.height = 28;
            items.push(button);
            y += 32;
        }
    }
    if let Some(field) = section.fields.iter().find(|field| field.field == SOLID_UI_SURFACE_ASSIGN) {
        y = push_field(items, field, y, options);
    }
    for field in &section.fields {
        if surface_factor(field.field) {
            y = push_field(items, field, y, options);
        }
    }
    if options.show_commands {
        for command in &section.commands {
            if *command == InspectorCommand::MakeUnique || surface_group_command(*command) {
                continue;
            }
            let mut button = control(
                ControlClass::Button,
                InspectorBinding::Command { command: *command, type_id: section.type_id },
                command_label(*command).into(),
                12,
                y,
            );
            button.face = ControlFace::Row;
            button.height = 24;
            items.push(button);
            y += 28;
        }
    }
    for field in &section.fields {
        if surface_group_field(field.field) {
            y = push_field(items, field, y, options);
        }
    }
    if options.show_commands {
        for command in &section.commands {
            if !surface_group_command(*command) {
                continue;
            }
            let mut button = control(
                ControlClass::Button,
                InspectorBinding::Command { command: *command, type_id: section.type_id },
                command_label(*command).into(),
                12,
                y,
            );
            button.face = ControlFace::Row;
            button.height = 24;
            items.push(button);
            y += 28;
        }
    }
    y
}

fn push_shelf(items: &mut Vec<PlannedControl>, section: &InspectorSection, y: &mut i32) {
    if section.title == "Select" {
        let chips = section
            .feature_edits
            .iter()
            .map(|(command, label, active)| (InspectorBinding::Command { command: *command, type_id: section.type_id }, label.clone(), *active))
            .collect::<Vec<_>>();
        let tools = section
            .commands
            .iter()
            .filter(|command| !command_in_header(**command))
            .map(|command| (InspectorBinding::Command { command: *command, type_id: section.type_id }, command_label(*command).to_string(), false))
            .collect::<Vec<_>>();
        push_chip_grid(items, y, &chips, ControlFace::Chip, 5, 28);
        push_chip_grid(items, y, &tools, ControlFace::Chip, 3, 28);
        return;
    }
    let (face, columns, height) = match section.title.as_str() {
        "Model" => (ControlFace::Tile, 2u8, 56i32),
        "Active Tool" => (ControlFace::Chip, 2, 28),
        _ => (ControlFace::Row, 0, 24),
    };
    let mut slots = Vec::new();
    for command in &section.commands {
        if command_in_header(*command) {
            continue;
        }
        slots.push((InspectorBinding::Command { command: *command, type_id: section.type_id }, command_label(*command).to_string(), false));
    }
    for (command, label, active) in &section.feature_edits {
        slots.push((InspectorBinding::Command { command: *command, type_id: section.type_id }, label.clone(), *active));
    }
    if slots.is_empty() {
        return;
    }
    if columns == 0 {
        for (binding, text, active) in slots {
            let mut button = control(ControlClass::Button, binding, text, 12, *y);
            button.face = face;
            button.height = height;
            button.checked = active;
            items.push(button);
            *y += height + 4;
        }
        return;
    }
    let width = columns as usize;
    let count = slots.len();
    for (index, (binding, text, active)) in slots.into_iter().enumerate() {
        let column = (index % width) as u8;
        let mut button = control(ControlClass::Button, binding, text, 8, *y);
        button.stretch = false;
        button.face = face;
        button.row_columns = columns;
        button.row_index = column;
        button.height = height;
        button.checked = active;
        items.push(button);
        if column + 1 == columns || index + 1 == count {
            *y += height + 4;
        }
    }
}

fn push_chip_grid(items: &mut Vec<PlannedControl>, y: &mut i32, slots: &[(InspectorBinding, String, bool)], face: ControlFace, columns: u8, height: i32) {
    if slots.is_empty() || columns == 0 {
        return;
    }
    let width = columns as usize;
    let count = slots.len();
    for (index, (binding, text, active)) in slots.iter().enumerate() {
        let column = (index % width) as u8;
        let mut button = control(ControlClass::Button, binding.clone(), text.clone(), 8, *y);
        button.stretch = false;
        button.face = face;
        button.row_columns = columns;
        button.row_index = column;
        button.height = height;
        button.checked = *active;
        items.push(button);
        if column + 1 == columns || index + 1 == count {
            *y += height + 4;
        }
    }
}

fn command_in_header(command: InspectorCommand) -> bool {
    matches!(
        command,
        InspectorCommand::RemoveComponent | InspectorCommand::ResetTransform | InspectorCommand::ResetMaterial | InspectorCommand::ResetPose
    )
}

pub fn command_label(command: InspectorCommand) -> &'static str {
    match command {
        InspectorCommand::RemoveComponent => "Remove",
        InspectorCommand::ResetTransform => "Reset",
        InspectorCommand::ResetMaterial => "Reset",
        InspectorCommand::SetStartupCamera => "Set as Startup Camera",
        InspectorCommand::Recapture => "Recapture",
        InspectorCommand::ResetPose => "Reset Pose",
        InspectorCommand::CreateTerrain => "Create Terrain",
        InspectorCommand::ResetShape => "Reset Shape",
        InspectorCommand::ExtrudeFace => "Extrude",
        InspectorCommand::InsetFace => "Inset",
        InspectorCommand::Bevel => "Bevel",
        InspectorCommand::Round => "Round",
        InspectorCommand::CancelModeling => "Cancel",
        InspectorCommand::ApplyModeling => "Done",
        InspectorCommand::ApplyRound => "Apply",
        InspectorCommand::MirrorX => "Mirror X",
        InspectorCommand::MirrorY => "Mirror Y",
        InspectorCommand::MirrorZ => "Mirror Z",
        InspectorCommand::Duplicate => "Duplicate",
        InspectorCommand::AlignToGround => "Align",
        InspectorCommand::SnapTranslation => "Snap",
        InspectorCommand::EditBevel => "Bevel",
        InspectorCommand::EditInset(_) => "Inset",
        InspectorCommand::SelectionAuto => "Auto",
        InspectorCommand::SelectionObject => "Object",
        InspectorCommand::SelectionFace => "Face",
        InspectorCommand::SelectionEdge => "Edge",
        InspectorCommand::SelectionVertex => "Vertex",
        InspectorCommand::SelectLoop => "Loop",
        InspectorCommand::SelectRing => "Ring",
        InspectorCommand::SelectConnected => "Connected",
        InspectorCommand::SelectBoundary => "Boundary",
        InspectorCommand::SelectGrow => "Grow",
        InspectorCommand::SelectShrink => "Shrink",
        InspectorCommand::MoveEdge => "Move Edge",
        InspectorCommand::ExtrudeEdge => "Extrude Edge",
        InspectorCommand::SplitEdge => "Split Edge",
        InspectorCommand::MoveVertex => "Move",
        InspectorCommand::SubdivideFace => "Subdivide",
        InspectorCommand::SubdivideFace4 => "4×4",
        InspectorCommand::AddMaterialSlot => "New Slot",
        InspectorCommand::MakeUnique => "Make Unique",
        InspectorCommand::SelectSlotFaces => "Select Faces Using Material",
        InspectorCommand::CreateSurfaceGroup => "Create Group",
        InspectorCommand::RenameSurfaceGroup => "Rename Group",
        InspectorCommand::AddSurfaceGroupFaces => "Add To Group",
        InspectorCommand::RemoveSurfaceGroupFaces => "Remove From Group",
        InspectorCommand::SelectSurfaceGroup => "Select Group",
        InspectorCommand::DeleteSurfaceGroup => "Delete Group",
    }
}

/// Editor-only face label. Not a registry field and not saved.
pub const SOLID_UI_FACE: FieldId = FieldId(240);
/// Editor-only amount for the open modeling operation. Not a registry field.
pub const SOLID_UI_AMOUNT: FieldId = FieldId(241);
/// Current block size readout. Not submitted through SetProperty.
pub const SOLID_UI_BLOCK: FieldId = FieldId(242);
/// Current bevel readout. Not submitted through SetProperty.
pub const SOLID_UI_BEVEL: FieldId = FieldId(243);
/// Edge length for the selected topological edge. Not a registry field.
pub const SOLID_UI_LENGTH: FieldId = FieldId(244);
/// Area of the selected face, square meters. Not a registry field.
pub const SOLID_UI_AREA: FieldId = FieldId(252);
/// Outward normal of the selected face. Not a registry field.
pub const SOLID_UI_NORMAL: FieldId = FieldId(253);
/// Stored or analytic face count. Not a registry field.
pub const SOLID_UI_FACES: FieldId = FieldId(254);
/// Stored or analytic edge count. Not a registry field.
pub const SOLID_UI_EDGES: FieldId = FieldId(255);
/// Stored or analytic vertex count. Not a registry field.
pub const SOLID_UI_VERTS: FieldId = FieldId(256);
/// Closed-solid readout. A stored body and the analytic box are closed.
pub const SOLID_UI_CLOSED: FieldId = FieldId(257);
/// Show Grid check. The plan reads [`PlanOptions::show_grid`]. Not a registry field.
pub const SOLID_UI_GRID: FieldId = FieldId(258);
/// Name of the open modeling operation. Not a registry field.
pub const SOLID_UI_TOOL: FieldId = FieldId(259);
/// Red channel of the surface base color, 0 to 1. Not a registry field.
pub const SOLID_UI_COLOR_R: FieldId = FieldId(270);
/// Green channel of the surface base color, 0 to 1. Not a registry field.
pub const SOLID_UI_COLOR_G: FieldId = FieldId(271);
/// Blue channel of the surface base color, 0 to 1. Not a registry field.
pub const SOLID_UI_COLOR_B: FieldId = FieldId(272);
/// Perceptual roughness of the surface slot, 0 to 1. Not a registry field.
pub const SOLID_UI_ROUGHNESS: FieldId = FieldId(273);
/// Metallic factor of the surface slot, 0 to 1. Not a registry field.
pub const SOLID_UI_METALLIC: FieldId = FieldId(274);
/// How many faces the Surface card is editing. Not a registry field.
pub const SOLID_UI_SURFACE_COUNT: FieldId = FieldId(275);
/// Name of the material those faces share. Not a registry field.
pub const SOLID_UI_SURFACE_SLOT: FieldId = FieldId(276);
/// What a color edit will do to the rest of the solid. Not a registry field.
pub const SOLID_UI_SURFACE_NOTE: FieldId = FieldId(277);
/// Existing slot chosen for the selected faces. Not a registry field.
pub const SOLID_UI_SURFACE_ASSIGN: FieldId = FieldId(278);
/// Group that contains the selected faces. Not a registry field.
pub const SOLID_UI_GROUP_MEMBERSHIP: FieldId = FieldId(279);
/// Visible name typed for Create Group or Rename Group. Not a registry field.
pub const SOLID_UI_GROUP_NAME: FieldId = FieldId(280);
/// Group chosen when no face is selected. Not a registry field.
pub const SOLID_UI_GROUP_PICK: FieldId = FieldId(281);
/// Round profile readout. Not a registry field and not authored.
pub const SOLID_UI_PROFILE: FieldId = FieldId(282);
/// Round render-resolution readout. Not a registry field and not authored.
pub const SOLID_UI_RESOLUTION: FieldId = FieldId(283);
/// Round radius interval. Not a registry field and not authored.
pub const SOLID_UI_RANGE: FieldId = FieldId(284);
/// How many semantic edges the picked topology resolved to. Not a registry field.
pub const SOLID_UI_RESOLVED: FieldId = FieldId(285);
/// Valid, Clamped, or Conflict. Not a registry field and not authored.
pub const SOLID_UI_STATUS: FieldId = FieldId(286);
/// Fragment collapse or the reason Apply will refuse. Not a registry field.
pub const SOLID_UI_ROUND_NOTE: FieldId = FieldId(287);
/// Object mode. Slot 0 is the material every unassigned face uses.
pub const SURFACE_OBJECT_NOTE: &str = "Every unassigned face uses this material.";
/// The selection is every face on slot 0, so a color edit is the whole solid.
pub const SURFACE_WHOLE_NOTE: &str = "Editing Object Material changes every face that uses it.";
/// The selection shares a slot with faces that are not selected.
pub const SURFACE_SHARED_NOTE: &str = "Shared with other faces. A color edit makes a material for this selection only.";
/// The selection is the only set of faces on this slot.
pub const SURFACE_OWN_NOTE: &str = "Only this selection uses this material.";
/// The selected faces name more than one slot.
pub const SURFACE_MIXED_NOTE: &str = "Those faces do not share a material. Make Unique copies Object Material.";
/// The selection is part of one group, and that group is the only user of its slot.
pub const SURFACE_GROUP_OWN_NOTE: &str = "Only this group uses this material. A color edit changes the whole group.";
/// The group's slot is still shared with faces that are not in the group.
pub const SURFACE_GROUP_SHARED_NOTE: &str = "Shared with other faces. A color edit makes a material for this group only.";

/// Current inset readout for one face. Not submitted through SetProperty.
pub fn solid_ui_inset(face: u8) -> FieldId {
    FieldId(245 + u32::from(face))
}

/// Terrain actor inspection when Land Mode is showing that actor instead of the tool row.
pub fn build_entity(world: &jarvig_core::SceneWorld, id: EntityUuid, staged: &[String], mesh_assets: &[String]) -> InspectorModel {
    let selection_revision = 0;
    let world_revision = world.revision();
    match world.inspect_entity(id) {
        Ok(inspection) => {
            let (header_name, header_kind) = headers(world, id);
            InspectorModel {
                selection_revision,
                world_revision,
                header_name,
                header_kind,
                body: InspectorBody::Entity { sections: sections_from(inspection, world, staged, mesh_assets, SolidElement::Object, "Auto") },
            }
        }
        Err(_) => InspectorModel::empty(),
    }
}

/// Editor-local size form. These numbers are not a terrain until Create Terrain runs.
pub fn terrain_draft(width: f64, depth: f64, spacing: f64, chunk: f64, height: f64) -> InspectorModel {
    let size = |field, label, value| draft_number(field, label, value, Some(0.01));
    InspectorModel {
        selection_revision: 0,
        world_revision: 0,
        header_name: "Create Terrain".into(),
        header_kind: "Terrain".into(),
        body: InspectorBody::Entity {
            sections: vec![InspectorSection {
                title: "Terrain".into(),
                type_id: TYPE_TERRAIN,
                fields: vec![
                    size(jarvig_core::FIELD_TERRAIN_WIDTH, "Width", width),
                    size(jarvig_core::FIELD_TERRAIN_DEPTH, "Depth", depth),
                    size(jarvig_core::FIELD_TERRAIN_SPACING, "Meters Per Vertex", spacing),
                    size(jarvig_core::FIELD_TERRAIN_CHUNK, "Chunk Size", chunk),
                    draft_number(jarvig_core::FIELD_TERRAIN_HEIGHT, "Height", height, None),
                ],
                commands: vec![InspectorCommand::CreateTerrain],
                feature_edits: Vec::new(),
            }],
        },
    }
}

/// Editor-only Land controls. These field ids are not in the type registry and are not saved.
pub const LAND_UI_GRID: FieldId = FieldId(220);
pub const LAND_UI_MINOR: FieldId = FieldId(221);
pub const LAND_UI_MAJOR: FieldId = FieldId(222);
pub const LAND_UI_SNAP: FieldId = FieldId(224);
pub const LAND_UI_RADIUS: FieldId = FieldId(225);
pub const LAND_UI_STRENGTH: FieldId = FieldId(226);
pub const LAND_UI_FALLOFF: FieldId = FieldId(227);
pub const LAND_UI_WORLD: FieldId = FieldId(228);
pub const LAND_UI_VERTS: FieldId = FieldId(229);
pub const LAND_UI_CHUNKS: FieldId = FieldId(230);
pub const LAND_UI_LOD: FieldId = FieldId(231);
pub const LAND_UI_SHOW_TERRAIN: FieldId = FieldId(232);
pub const LAND_UI_SHOW_HELPERS: FieldId = FieldId(233);
pub const LAND_UI_SHOW_LIGHTING: FieldId = FieldId(234);
pub const LAND_UI_SHOW_CHARACTERS: FieldId = FieldId(235);
pub const LAND_UI_SHOW_PROPS: FieldId = FieldId(236);
pub const LAND_UI_SHOW_GAMEPLAY: FieldId = FieldId(237);
pub const LAND_UI_SHOW_FULL: FieldId = FieldId(238);
/// Writes the player definition file. It is not a level field.
pub const PLAYER_UI_CHARACTER: FieldId = FieldId(239);

pub fn is_land_ui(field: FieldId) -> bool {
    (220..=238).contains(&field.0)
}

#[derive(Clone, Copy, Debug)]
pub struct LandInspector {
    pub grid: bool,
    pub minor: f64,
    pub major: f64,
    pub snap: bool,
    pub radius: f64,
    pub strength: f64,
    pub falloff: &'static str,
    pub world: bool,
    pub vertices: bool,
    pub chunks: bool,
    pub lod: bool,
    pub show_terrain: bool,
    pub show_helpers: bool,
    pub show_lighting: bool,
    pub show_characters: bool,
    pub show_props: bool,
    pub show_gameplay: bool,
    pub show_full: bool,
}

/// Grid, brush, and debug overlay. Editor state. Not a terrain component and not a level entity.
pub fn attach_land_workspace(model: &mut InspectorModel, land: LandInspector) {
    let InspectorBody::Entity { sections } = &mut model.body else { return };
    sections.push(InspectorSection {
        title: "Grid".into(),
        type_id: TYPE_TERRAIN,
        fields: vec![
            land_check(LAND_UI_GRID, "Grid Overlay", land.grid),
            land_check(LAND_UI_WORLD, "World Units", land.world),
            land_number(LAND_UI_MINOR, "Minor Spacing", land.minor, Some(0.05)),
            land_number(LAND_UI_MAJOR, "Major Spacing", land.major, Some(0.05)),
            land_check(LAND_UI_SNAP, "Snap", land.snap),
        ],
        commands: Vec::new(),
        feature_edits: Vec::new(),
    });
    sections.push(InspectorSection {
        title: "Brush".into(),
        type_id: TYPE_TERRAIN,
        fields: vec![
            land_number(LAND_UI_RADIUS, "Radius", land.radius, Some(0.05)),
            land_number(LAND_UI_STRENGTH, "Strength", land.strength, Some(0.0)),
            land_choice(LAND_UI_FALLOFF, "Falloff", land.falloff, &["Smooth", "Linear"]),
        ],
        commands: Vec::new(),
        feature_edits: Vec::new(),
    });
    sections.push(InspectorSection {
        title: "Debug Overlay".into(),
        type_id: TYPE_TERRAIN,
        fields: vec![
            land_check(LAND_UI_VERTS, "Vertex Dots", land.vertices),
            land_check(LAND_UI_CHUNKS, "Chunk Boundaries", land.chunks),
            land_check(LAND_UI_LOD, "LOD Boundaries", land.lod),
        ],
        commands: Vec::new(),
        feature_edits: Vec::new(),
    });
    sections.push(InspectorSection {
        title: "Visibility".into(),
        type_id: TYPE_TERRAIN,
        fields: vec![
            land_check(LAND_UI_SHOW_TERRAIN, "Terrain", land.show_terrain),
            land_check(LAND_UI_SHOW_HELPERS, "Landscape Helpers", land.show_helpers),
            land_check(LAND_UI_SHOW_LIGHTING, "Lighting", land.show_lighting),
            land_check(LAND_UI_SHOW_CHARACTERS, "Characters", land.show_characters),
            land_check(LAND_UI_SHOW_PROPS, "Props", land.show_props),
            land_check(LAND_UI_SHOW_GAMEPLAY, "Gameplay Actors", land.show_gameplay),
            land_check(LAND_UI_SHOW_FULL, "Show Full Level", land.show_full),
        ],
        commands: Vec::new(),
        feature_edits: Vec::new(),
    });
}

fn land_check(field: FieldId, label: &str, checked: bool) -> InspectorField {
    InspectorField {
        type_id: TYPE_TERRAIN,
        field,
        label: label.into(),
        units: String::new(),
        display: if checked { "true".into() } else { "false".into() },
        components: Vec::new(),
        editable: true,
        kind: ValueKind::Bool,
        widget: WidgetKind::Check,
        minimum: None,
        maximum: None,
        precision: 0,
        step: 0.0,
        choices: Vec::new(),
        asset_type: String::new(),
    }
}

fn land_number(field: FieldId, label: &str, value: f64, minimum: Option<f64>) -> InspectorField {
    InspectorField {
        type_id: TYPE_TERRAIN,
        field,
        label: label.into(),
        units: "m".into(),
        display: format!("{value}"),
        components: Vec::new(),
        editable: true,
        kind: ValueKind::Float64,
        widget: WidgetKind::Number,
        minimum,
        maximum: None,
        precision: 3,
        step: 0.1,
        choices: Vec::new(),
        asset_type: String::new(),
    }
}

fn land_choice(field: FieldId, label: &str, selected: &str, choices: &[&str]) -> InspectorField {
    InspectorField {
        type_id: TYPE_TERRAIN,
        field,
        label: label.into(),
        units: String::new(),
        display: selected.into(),
        components: Vec::new(),
        editable: true,
        kind: ValueKind::String,
        widget: WidgetKind::Choice,
        minimum: None,
        maximum: None,
        precision: 0,
        step: 0.0,
        choices: choices.iter().map(|choice| (*choice).to_string()).collect(),
        asset_type: String::new(),
    }
}

fn draft_number(field: FieldId, label: &str, value: f64, minimum: Option<f64>) -> InspectorField {
    InspectorField {
        type_id: TYPE_TERRAIN,
        field,
        label: label.into(),
        units: "m".into(),
        display: format!("{value}"),
        components: Vec::new(),
        editable: true,
        kind: ValueKind::Float64,
        widget: WidgetKind::Number,
        minimum,
        maximum: None,
        precision: 3,
        step: 1.0,
        choices: Vec::new(),
        asset_type: String::new(),
    }
}

/// Parent and child names for the selected joint. Display only. The pose stays the local frame.
pub fn attach_joint_context(model: &mut InspectorModel, world: &jarvig_core::SceneWorld, entity: EntityUuid) {
    let Some(joint) = world.authored_joint(entity) else { return };
    model.header_kind = format!("{} joint", joint.kind.label());
    let InspectorBody::Entity { sections } = &mut model.body else { return };
    let parent = world
        .entity_parent(entity)
        .ok()
        .flatten()
        .and_then(|id| world.entity_outline().into_iter().find(|row| row.uuid == id).map(|row| row.name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "World".into());
    let children: Vec<String> = world
        .entity_outline()
        .into_iter()
        .filter(|row| row.parent == Some(entity))
        .map(|row| if row.name.is_empty() { "Unnamed".into() } else { row.name })
        .collect();
    let child = if children.is_empty() { "None".into() } else { children.join(", ") };
    let kind = joint.kind;
    if let Some(section) = sections.iter_mut().find(|section| section.title == "Joint") {
        section.fields.retain(|field| match kind {
            jarvig_core::JointKind::Fixed => !matches!(field.label.as_str(), "Axis" | "Secondary Axis"),
            jarvig_core::JointKind::Universal => true,
            _ => field.label != "Secondary Axis",
        });
        section.fields.insert(1, readonly_line(jarvig_core::FieldId(6701), "Parent", parent));
        section.fields.insert(2, readonly_line(jarvig_core::FieldId(6702), "Child", child));
        section.commands.insert(0, InspectorCommand::ResetPose);
    }
    if let Some(section) = sections.iter_mut().find(|section| section.title == "Limits") {
        section.fields.retain(|field| limit_applies(kind, &field.label));
    }
    sections.retain(|section| section.title != "Limits" || !section.fields.is_empty());
    let rank = |title: &str| match title {
        "Joint" => 0,
        "Limits" => 1,
        "Actor" => 2,
        "Transform" => 3,
        "Mesh Renderer" => 4,
        _ => 5,
    };
    sections.sort_by_key(|section| rank(&section.title));
}

/// Player Start choices. The character choice writes the player file, not the level.
pub fn attach_player_start(model: &mut InspectorModel, world: &jarvig_core::SceneWorld, entity: EntityUuid, players: &[String], characters: &[String], character: &str) {
    let Some(start) = world.authored_player_start(entity) else { return };
    let InspectorBody::Entity { sections } = &mut model.body else { return };
    let Some(section) = sections.iter_mut().find(|section| section.title == "Player Start") else { return };
    if let Some(field) = section.fields.iter_mut().find(|field| field.field == jarvig_core::FIELD_PLAYER_DEFINITION) {
        field.widget = WidgetKind::Choice;
        field.editable = true;
        field.kind = ValueKind::String;
        field.choices = players.to_vec();
        if !start.player.is_empty() && !field.choices.iter().any(|choice| choice == &start.player) {
            field.choices.insert(0, start.player.clone());
        }
        field.display = start.player.clone();
    }
    let mut choices = characters.to_vec();
    if !character.is_empty() && !choices.iter().any(|choice| choice == character) {
        choices.insert(0, character.to_string());
    }
    section.fields.push(InspectorField {
        type_id: jarvig_core::TYPE_PLAYER_START,
        field: PLAYER_UI_CHARACTER,
        label: "Character".into(),
        units: String::new(),
        display: character.to_string(),
        components: Vec::new(),
        editable: !start.player.is_empty(),
        kind: ValueKind::String,
        widget: WidgetKind::Choice,
        minimum: None,
        maximum: None,
        precision: 0,
        step: 0.0,
        choices,
        asset_type: String::new(),
    });
}

fn limit_applies(kind: jarvig_core::JointKind, label: &str) -> bool {
    match kind {
        jarvig_core::JointKind::Fixed => false,
        jarvig_core::JointKind::Hinge => matches!(label, "Hinge Min" | "Hinge Max"),
        jarvig_core::JointKind::Ball => matches!(label, "Swing Cone" | "Twist Min" | "Twist Max"),
        jarvig_core::JointKind::Universal => matches!(label, "Primary Min" | "Primary Max" | "Secondary Min" | "Secondary Max"),
        jarvig_core::JointKind::Prismatic => matches!(label, "Slide Min" | "Slide Max"),
    }
}

fn readonly_line(field: FieldId, label: &str, display: String) -> InspectorField {
    InspectorField {
        type_id: jarvig_core::TYPE_JOINT,
        field,
        label: label.to_string(),
        units: String::new(),
        display,
        components: Vec::new(),
        editable: false,
        kind: ValueKind::String,
        widget: WidgetKind::Readonly,
        minimum: None,
        maximum: None,
        precision: 0,
        step: 0.0,
        choices: Vec::new(),
        asset_type: String::new(),
    }
}

fn push_field(items: &mut Vec<PlannedControl>, field: &InspectorField, mut y: i32, options: &PlanOptions) -> i32 {
    let enabled = field.editable;
    match field.widget {
        WidgetKind::Check => {
            let grid = field.field == SOLID_UI_GRID;
            let binding = if grid {
                InspectorBinding::ShowGrid
            } else {
                InspectorBinding::Check { type_id: field.type_id, field: field.field }
            };
            let mut row = control(ControlClass::Check, binding, field.label.clone(), 12, y);
            row.checked = if grid { options.show_grid } else { matches!(field.display.as_str(), "true" | "1") };
            row.enabled = enabled;
            items.push(row);
            y + 24
        }
        WidgetKind::Choice => {
            let mut label = control(ControlClass::Label, InspectorBinding::Label, field.label.clone(), 12, y);
            label.stretch = false;
            label.width = 112;
            items.push(label);
            let browse = !field.asset_type.is_empty();
            let mut combo = control(ControlClass::Combo, InspectorBinding::Choice { type_id: field.type_id, field: field.field }, field.display.clone(), 132, y);
            combo.choices = field.choices.clone();
            combo.selected = selected_choice(field);
            combo.enabled = enabled;
            combo.height = 180;
            if browse {
                combo.right_gutter = 34;
            }
            items.push(combo);
            if browse {
                let mut button = control(ControlClass::Button, InspectorBinding::Browse { type_id: field.type_id, field: field.field }, "…".into(), 0, y);
                button.stretch = false;
                button.width = 28;
                button.right_slot = Some(0);
                button.enabled = enabled;
                items.push(button);
            }
            y + 24
        }
        WidgetKind::Vector | WidgetKind::Euler | WidgetKind::Color => {
            items.push(control(ControlClass::Label, InspectorBinding::Label, field.label.clone(), 12, y));
            y += 22;
            let letters = if field.widget == WidgetKind::Color { ["R", "G", "B"] } else { ["X", "Y", "Z"] };
            let units = if field.widget == WidgetKind::Color { "" } else { unit_text(&field.units) };
            for axis in 0..3 {
                let mut axis_label = control(ControlClass::Label, InspectorBinding::Label, letters[axis].into(), 24, y);
                axis_label.stretch = false;
                axis_label.width = 16;
                items.push(axis_label);
                let mut edit = control(
                    ControlClass::Edit,
                    InspectorBinding::Axis { type_id: field.type_id, field: field.field, axis: axis as u8 },
                    field.components.get(axis).cloned().unwrap_or_default(),
                    44,
                    y,
                );
                edit.stretch = false;
                edit.width = 88;
                edit.enabled = enabled;
                items.push(edit);
                if !units.is_empty() {
                    let mut unit = control(ControlClass::Label, InspectorBinding::Label, units.into(), 138, y);
                    unit.stretch = false;
                    unit.width = 48;
                    items.push(unit);
                }
                y += 24;
            }
            if field.field == FIELD_OBJECT_SCALE {
                let mut lock = control(ControlClass::Check, InspectorBinding::Uniform, "Uniform scale".into(), 12, y);
                lock.checked = options.uniform_scale;
                lock.enabled = enabled;
                items.push(lock);
                y += 24;
            }
            if field.widget == WidgetKind::Color && enabled {
                items.push(control(ControlClass::Button, InspectorBinding::Color { type_id: field.type_id, field: field.field }, "Pick color".into(), 12, y));
                y += 24;
            }
            y
        }
        WidgetKind::Number | WidgetKind::Text => {
            let mut label = control(ControlClass::Label, InspectorBinding::Label, field.label.clone(), 12, y);
            label.stretch = false;
            label.width = 112;
            items.push(label);
            let text = if field.widget == WidgetKind::Number { shown_number(field) } else { field.display.clone() };
            let mut edit = control(ControlClass::Edit, InspectorBinding::Text { type_id: field.type_id, field: field.field }, text, 132, y);
            edit.enabled = enabled;
            if !field.units.is_empty() && field.widget == WidgetKind::Number {
                edit.right_gutter = 40;
            }
            items.push(edit);
            if !field.units.is_empty() && field.widget == WidgetKind::Number {
                let mut unit = control(ControlClass::Label, InspectorBinding::Label, unit_text(&field.units).into(), 0, y);
                unit.stretch = false;
                unit.width = 36;
                unit.right_slot = Some(0);
                items.push(unit);
            }
            y + 24
        }
        WidgetKind::Readonly => {
            let mut label = control(ControlClass::Label, InspectorBinding::Label, field.label.clone(), 12, y);
            label.stretch = false;
            label.width = 112;
            items.push(label);
            items.push(control(ControlClass::Label, InspectorBinding::Label, field.display.clone(), 132, y));
            y + 24
        }
    }
}

fn selected_choice(field: &InspectorField) -> usize {
    if field.kind == ValueKind::Float64 {
        if let Ok(number) = field.display.parse::<f64>() {
            if let Some(index) = field.choices.iter().position(|choice| choice_as_number(choice) == Some(number)) {
                return index;
            }
        }
    }
    field.choices.iter().position(|choice| choice == &field.display).unwrap_or(0)
}

fn shown_number(field: &InspectorField) -> String {
    match field.display.parse::<f64>() {
        Ok(value) if field.precision > 0 && value.is_finite() => format_fixed(value, field.precision),
        _ => field.display.clone(),
    }
}

fn unit_text(units: &str) -> &str {
    match units {
        "deg" => "°",
        "0 uniform, 1 log" => "",
        other => other,
    }
}

fn control(class: ControlClass, binding: InspectorBinding, text: String, x: i32, y: i32) -> PlannedControl {
    PlannedControl {
        class,
        binding,
        text,
        x,
        y,
        width: 0,
        height: 20,
        stretch: true,
        enabled: true,
        right_gutter: 0,
        right_slot: None,
        choices: Vec::new(),
        selected: 0,
        checked: false,
        face: ControlFace::Native,
        row_columns: 0,
        row_index: 0,
    }
}

fn format_fixed(value: f64, precision: u8) -> String {
    let precision = precision.min(6) as usize;
    let text = format!("{value:.precision$}");
    if text.starts_with("-0") && text.chars().skip(1).all(|glyph| glyph == '0' || glyph == '.') {
        text[1..].to_string()
    } else {
        text
    }
}

/// Label plus the registry units. Tests still use this. The plan draws units beside the number.
#[cfg_attr(not(test), allow(dead_code))]
pub fn field_heading(label: &str, units: &str) -> String {
    if units.is_empty() { label.to_string() } else { format!("{label} ({units})") }
}

/// `Ok(None)` means the text still names the current value, so no command is sent.
#[cfg_attr(not(test), allow(dead_code))]
pub fn property_from_text(kind: ValueKind, text: &str, current_display: &str) -> Result<Option<PropertyValue>, &'static str> {
    if text == current_display {
        return Ok(None);
    }
    match kind {
        ValueKind::String => Ok(Some(PropertyValue::String(text.to_string()))),
        ValueKind::Float64 => {
            let value = text.trim().parse::<f64>().ok().filter(|value| value.is_finite()).ok_or("the text was not a finite number")?;
            if current_display.parse::<f64>().ok() == Some(value) {
                return Ok(None);
            }
            Ok(Some(PropertyValue::F64(value)))
        }
        ValueKind::Bool => {
            let value = parse_bool(text).ok_or("the text was not true or false")?;
            let current = parse_bool(current_display).ok_or("the text was not true or false")?;
            if value == current {
                return Ok(None);
            }
            Ok(Some(PropertyValue::Bool(value)))
        }
        _ => Err("this field is not edited as one text value"),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "on" | "yes" => Some(true),
        "false" | "0" | "off" | "no" => Some(false),
        _ => None,
    }
}

/// Same command path the property controls use. Tests and the window both call this.
pub fn submit_property(engine: &mut EngineSession, target: EntityUuid, type_id: TypeId, field: FieldId, value: PropertyValue) -> Result<jarvig_core::AuthoringResult, jarvig_core::AuthoringError> {
    engine.execute_authoring(jarvig_engine::AuthoringCommand::SetProperty { target, type_id, field, value })
}

/// Degrees in, the same quaternion the rotate gizmo writes.
pub fn quat_from_degrees(degrees: [f64; 3]) -> Quat {
    Quat::from_euler_xyz_degrees(jarvig_core::Vec3::new(degrees[0], degrees[1], degrees[2]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvig_core::{
        AuthoringResult, PropertyValue, SceneWorld, FIELD_LOCAL_ROTATION, FIELD_LOCAL_TRANSLATION, FIELD_NAME, FIELD_PARENT, FIELD_UUID,
        TYPE_ENTITY, TYPE_SPATIAL_FRAME,
    };
    use jarvig_engine::EngineSession;

    #[test]
    fn one_entity_builds_sections_and_a_group_does_not_edit() {
        let world = SceneWorld::bootstrap();
        let outline = world.entity_outline();
        let near = outline[0].uuid;
        let far = outline[1].uuid;
        let mut selection = SelectionService::default();
        assert!(matches!(build(&selection, &world).body, InspectorBody::Empty));
        selection.replace(SelectionItem::entity(near).unwrap()).unwrap();
        let model = build(&selection, &world);
        assert_eq!(
            model.section_titles(),
            ["Actor", "Advanced", "Transform", "Mesh Renderer", "Materials", "Material Instance", "Components"]
        );
        assert_eq!(model.field_count(), 16);
        assert_eq!(model.header_name, "Near Triangle");
        assert_eq!(model.header_kind, "Mesh Actor");
        let expected = world.component_stack(near).unwrap().iter().map(|item| item.role.label()).collect::<Vec<_>>().join(", ");
        assert_eq!(model.field(jarvig_core::TYPE_COMPONENT_STACK, jarvig_core::FIELD_COMPONENT_STACK).unwrap().display, expected);
        assert!(!model.field(jarvig_core::TYPE_COMPONENT_STACK, jarvig_core::FIELD_COMPONENT_STACK).unwrap().editable);
        assert!(model.field(TYPE_ENTITY, FIELD_NAME).unwrap().editable);
        assert!(!model.field(TYPE_ENTITY, FIELD_UUID).unwrap().editable);
        assert_eq!(model.field(TYPE_ENTITY, FIELD_PARENT).unwrap().display, "World");
        assert!(model.field(TYPE_ENTITY, FIELD_PARENT).unwrap().editable);
        assert_eq!(model.field(TYPE_SPATIAL_FRAME, FIELD_LOCAL_TRANSLATION).unwrap().components, ["0.000", "0.000", "-2.000"]);
        let rotation = model.field(TYPE_SPATIAL_FRAME, FIELD_LOCAL_ROTATION).unwrap();
        assert!(rotation.editable);
        assert_eq!(rotation.components, ["0.00", "0.00", "0.00"]);
        assert_eq!(model.field(TYPE_MESH_RENDERER, FIELD_OBJECT_SCALE).unwrap().components, ["1.000", "1.000", "1.000"]);
        let surface = model.field(TYPE_MESH_RENDERER, FIELD_SURFACE).unwrap();
        assert!(!surface.editable);
        assert_eq!(surface.display, "Triangle");
        selection.add(SelectionItem::entity(far).unwrap()).unwrap();
        match build(&selection, &world).body {
            InspectorBody::Multiple { count, primary_name } => {
                assert_eq!(count, 2);
                assert_eq!(primary_name, "Far Triangle");
            }
            _ => panic!("multi-selection must not open one entity form"),
        }
    }

    #[test]
    fn the_plan_draws_controls_and_hides_the_command_sentence() {
        let world = SceneWorld::bootstrap();
        let near = world.entity_outline()[0].uuid;
        let mut selection = SelectionService::default();
        selection.replace(SelectionItem::entity(near).unwrap()).unwrap();
        let model = build(&selection, &world);
        let controls = plan(&model, &PlanOptions::editing());
        let texts: Vec<_> = controls.iter().map(|control| control.text.as_str()).collect();
        assert!(texts.contains(&"NEAR TRIANGLE"));
        assert!(texts.contains(&"Mesh Actor"));
        assert!(!texts.iter().any(|text| text.contains("Edits are engine commands")));
        assert!(controls.iter().any(|control| control.text == "Cast Shadows" && control.class == ControlClass::Check && control.enabled));
        assert!(controls.iter().any(|control| matches!(control.binding, InspectorBinding::Axis { field, axis: 0, .. } if field == FIELD_OBJECT_SCALE)));
        assert!(controls.iter().any(|control| matches!(control.binding, InspectorBinding::Axis { field, .. } if field == FIELD_LOCAL_ROTATION)));
        assert!(texts.contains(&"+ Add Component"));
        assert!(!texts.iter().any(|text| *text == near.to_string()));
        let open = PlanOptions { advanced_open: true, ..PlanOptions::editing() };
        let advanced = plan(&model, &open);
        assert!(advanced.iter().any(|control| control.text == near.to_string()));
    }

    #[test]
    fn a_pose_change_keeps_the_inspector_control_layout() {
        let mut world = SceneWorld::bootstrap();
        let near = world.entity_outline()[0].uuid;
        let mut selection = SelectionService::default();
        selection.replace(SelectionItem::entity(near).unwrap()).unwrap();
        let before = build(&selection, &world);
        world.set_entity_local_translation(near, jarvig_core::Vec3::new(1.25, 0.0, -2.0)).unwrap();
        let after = build(&selection, &world);
        assert_ne!(before.world_revision, after.world_revision);
        let options = PlanOptions::editing();
        let left = plan(&before, &options);
        let right = plan(&after, &options);
        assert_eq!(left.len(), right.len());
        assert!(left.iter().zip(&right).all(|(left, right)| same_control_layout(left, right)));
        assert!(right.iter().any(|control| control.text == "1.250"));
        assert!(left.iter().any(|control| control.text == "0.000"));
    }

    #[test]
    fn the_command_path_updates_the_model_and_not_the_selection() {
        let mut engine = EngineSession::editor().unwrap();
        let near = engine.world().entity_outline()[0].uuid;
        let mut selection = SelectionService::default();
        selection.replace(SelectionItem::entity(near).unwrap()).unwrap();
        let selection_revision = selection.revision();
        let world_revision = engine.world().revision();
        assert_eq!(
            submit_property(&mut engine, near, TYPE_ENTITY, FIELD_NAME, PropertyValue::String("Player Start".into())).unwrap(),
            AuthoringResult::Applied
        );
        assert_eq!(submit_property(&mut engine, near, TYPE_ENTITY, FIELD_NAME, PropertyValue::String("Player Start".into())).unwrap(), AuthoringResult::Unchanged);
        assert_eq!(engine.world().revision(), world_revision + 1);
        assert_eq!(selection.revision(), selection_revision);
        assert_eq!(build(&selection, engine.world()).field(TYPE_ENTITY, FIELD_NAME).unwrap().display, "Player Start");
    }

    #[test]
    fn text_commits_keep_the_registry_type_and_skip_an_unchanged_value() {
        assert_eq!(property_from_text(ValueKind::Float64, "14", "14").unwrap(), None);
        assert_eq!(property_from_text(ValueKind::Float64, "14.0", "14").unwrap(), None);
        assert_eq!(property_from_text(ValueKind::Float64, "16", "14").unwrap(), Some(PropertyValue::F64(16.0)));
        assert!(property_from_text(ValueKind::Float64, "nope", "14").is_err());
        assert_eq!(property_from_text(ValueKind::Bool, "true", "true").unwrap(), None);
        assert_eq!(property_from_text(ValueKind::Bool, "1", "true").unwrap(), None);
        assert_eq!(property_from_text(ValueKind::Bool, "off", "true").unwrap(), Some(PropertyValue::Bool(false)));
        assert!(property_from_text(ValueKind::Bool, "maybe", "true").is_err());
        assert_eq!(property_from_text(ValueKind::String, "Blue", "Blue Point Light").unwrap(), Some(PropertyValue::String("Blue".into())));
        assert_eq!(field_heading("Intensity", "cd"), "Intensity (cd)");
        assert_eq!(field_heading("Enabled", ""), "Enabled");
        assert_eq!(field_heading("Location", "m"), "Location (m)");
        assert_eq!(choice_as_number("Default"), Some(0.0));
        assert_eq!(clamp_number(-1.0, Some(0.0), None), Some(0.0));
    }

    #[test]
    fn a_selected_light_and_world_settings_show_their_component_fields() {
        let world = SceneWorld::bootstrap();
        let outline = world.entity_outline();
        let point = outline.iter().find(|row| row.name == "Blue Point Light").unwrap().uuid;
        let spot = outline.iter().find(|row| row.name == "Warm Spot Light").unwrap().uuid;
        let settings = outline.iter().find(|row| row.name == "World Settings").unwrap().uuid;
        let probe = outline.iter().find(|row| row.name == "Reflection Probe").unwrap().uuid;
        let mut selection = SelectionService::default();
        selection.replace(SelectionItem::entity(point).unwrap()).unwrap();
        let model = build(&selection, &world);
        assert_eq!(model.section_titles(), ["Actor", "Advanced", "Transform", "Point Light", "Components"]);
        assert_eq!(model.header_kind, "Point Light");
        let intensity = model.field(jarvig_core::TYPE_POINT_LIGHT, jarvig_core::FIELD_INTENSITY).unwrap();
        assert!(intensity.editable);
        assert_eq!(intensity.display, "14");
        assert_eq!(intensity.units, "cd");
        assert_eq!(model.field(jarvig_core::TYPE_POINT_LIGHT, jarvig_core::FIELD_COLOR).unwrap().label, "Color");
        let point_plan = plan(&model, &PlanOptions::editing());
        assert!(point_plan.iter().any(|control| control.text == "Pick color"));
        assert!(point_plan.iter().any(|control| control.text == "Range"));
        assert!(!point_plan.iter().any(|control| control.text == "Inner Cone"));
        selection.replace(SelectionItem::entity(spot).unwrap()).unwrap();
        let model = build(&selection, &world);
        let spot_intensity = model.field(jarvig_core::TYPE_SPOT_LIGHT, jarvig_core::FIELD_INTENSITY).unwrap();
        assert_eq!(spot_intensity.display, "22");
        let spot_plan = plan(&model, &PlanOptions::editing());
        assert!(spot_plan.iter().any(|control| control.text == "Inner Cone"));
        assert!(spot_plan.iter().any(|control| control.text == "Outer Cone"));
        assert!(!spot_plan.iter().any(|control| control.text == "Cascade Count"));
        selection.replace(SelectionItem::entity(settings).unwrap()).unwrap();
        let model = build(&selection, &world);
        assert!(model.field(TYPE_SPATIAL_FRAME, FIELD_LOCAL_TRANSLATION).is_none());
        assert!(!model.field(TYPE_ENTITY, FIELD_PARENT).unwrap().editable);
        let sky = model.field(jarvig_core::TYPE_ENVIRONMENT, jarvig_core::FIELD_INTENSITY).unwrap();
        assert_eq!(sky.display, jarvig_core::format_f64(jarvig_core::BOOTSTRAP_ENVIRONMENT_INTENSITY as f64));
        assert!(sky.editable);
        selection.replace(SelectionItem::entity(probe).unwrap()).unwrap();
        let model = build(&selection, &world);
        assert_eq!(model.field(jarvig_core::TYPE_REFLECTION_PROBE, jarvig_core::FIELD_RADIUS).unwrap().display, "8");
        assert_eq!(model.field(jarvig_core::TYPE_REFLECTION_PROBE, jarvig_core::FIELD_RADIUS).unwrap().label, "Influence Size");
        assert!(model.field(jarvig_core::TYPE_REFLECTION_PROBE, jarvig_core::FIELD_UPDATE_MODE).unwrap().editable);
        assert!(model.field(jarvig_core::TYPE_REFLECTION_PROBE, jarvig_core::FIELD_RESOLUTION).unwrap().editable);
        assert!(!model.field(jarvig_core::TYPE_REFLECTION_PROBE, jarvig_core::FIELD_CAPTURE_STATE).unwrap().editable);
        assert_eq!(model.field(jarvig_core::TYPE_REFLECTION_PROBE, jarvig_core::FIELD_CAPTURE_STATE).unwrap().display, "Static. Edits do not recapture.");
        let probe_plan = plan(&model, &PlanOptions::editing());
        assert!(probe_plan.iter().any(|control| control.text == "Recapture"));
        assert!(!probe_plan.iter().any(|control| control.text.contains("do not recapture")));
    }

    #[test]
    fn add_component_lists_registry_types_and_skips_what_is_already_there() {
        let world = SceneWorld::bootstrap();
        let near = world.entity_outline()[0].uuid;
        let owned = owned_types(&world, near);
        let choices = add_component_choices(&owned);
        let names: Vec<_> = choices.iter().map(|(_, name)| *name).collect();
        assert!(names.contains(&"Camera"));
        assert!(names.contains(&"Point Light"));
        assert!(!names.iter().any(|name| *name == "Mesh Renderer" || *name == "Transform" || *name == "Environment"));
    }

    #[test]
    fn deleting_the_selected_light_clears_selection_and_the_inspector() {
        let mut engine = EngineSession::editor().unwrap();
        let compiles = engine.material_compile_count();
        let outline = engine.world().entity_outline();
        let point = outline.iter().find(|row| row.name == "Blue Point Light").unwrap().uuid;
        let near = outline[0].uuid;
        let mut selection = SelectionService::default();
        selection.replace_many(&[SelectionItem::entity(near).unwrap(), SelectionItem::entity(point).unwrap()]).unwrap();
        assert_eq!(selection.primary_entity(), Some(point));
        engine.execute_authoring(jarvig_engine::AuthoringCommand::DestroyEntity { target: point }).unwrap();
        let live: Vec<_> = engine.world().entity_outline().into_iter().map(|row| row.uuid).collect();
        selection.reconcile_entities(&live);
        assert_eq!(selection.primary_entity(), Some(near));
        assert!(!selection.contains(SelectionItem::Entity(point)));
        let rows = engine.world().entity_outline();
        let model = crate::outliner::WorldOutlinerModel::derive(engine.world().revision(), &rows, &crate::outliner::WorldOutlinerModel::empty());
        assert!(model.node(point).is_none());
        selection.replace(SelectionItem::entity(near).unwrap()).unwrap();
        engine.execute_authoring(jarvig_engine::AuthoringCommand::DestroyEntity { target: near }).unwrap();
        let live: Vec<_> = engine.world().entity_outline().into_iter().map(|row| row.uuid).collect();
        selection.reconcile_entities(&live);
        assert!(selection.is_empty());
        assert!(matches!(build(&selection, engine.world()).body, InspectorBody::Empty));
        assert_eq!(engine.material_compile_count(), compiles);
        assert_eq!(engine.world().light_count(), 2);
        assert_eq!(engine.world().object_count(), 1);
    }

    #[test]
    fn a_block_shows_modeling_tools_and_a_light_does_not() {
        let mut world = SceneWorld::bootstrap();
        let light = world.entity_outline().iter().find(|row| row.name == "Blue Point Light").unwrap().uuid;
        let block = world.create_block(jarvig_core::Vec3::new(0.0, 1.0, -4.0), jarvig_core::BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let mut selection = SelectionService::default();
        selection.replace(SelectionItem::entity(light).unwrap()).unwrap();
        let light_model = build(&selection, &world);
        assert_eq!(light_model.section_titles(), ["Actor", "Advanced", "Transform", "Point Light", "Components"]);
        let light_plan = plan(&light_model, &PlanOptions::editing());
        assert!(!light_plan.iter().any(|control| matches!(control.text.as_str(), "Extrude" | "Inset" | "Mirror X" | "Bevel")));
        selection.replace(SelectionItem::entity(block).unwrap()).unwrap();
        let mut model = build(&selection, &world);
        let titles = model.section_titles();
        for title in ["Dimensions", "History", "Select", "Model", "Geometry", "Collision", "Material"] {
            assert!(titles.iter().any(|existing| *existing == title), "missing {title} in {titles:?}");
        }
        assert!(!titles.iter().any(|existing| {
            matches!(*existing, "Shape" | "Modeling" | "Pattern" | "Placement" | "Face Tools" | "Object Tools" | "Features" | "Selection")
        }));
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_ORIGIN).unwrap().display, "Center");
        assert!(!model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_ORIGIN).unwrap().editable);
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_SIZE_X).unwrap().editable);
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_BEVEL).is_none());
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_INSET_PX).is_none());
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_HISTORY).is_none());
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_BLOCK).unwrap().display, "2.000 × 2.000 × 2.000 m");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_COLLISION).unwrap().display, "Analytic box");
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).is_none());
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACES).unwrap().display, "6");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_EDGES).unwrap().display, "12");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_VERTS).unwrap().display, "8");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_CLOSED).unwrap().display, "Yes");
        let planned = plan(&model, &PlanOptions::editing());
        for label in ["Reset Shape", "Bevel", "Duplicate", "Mirror X", "Mirror Y", "Mirror Z", "Align", "Snap", "Auto", "Object", "Face", "Edge", "Vert"] {
            assert!(planned.iter().any(|control| control.text == label), "missing {label}");
        }
        let auto = planned.iter().find(|control| control.text == "Auto").unwrap();
        assert!(auto.checked);
        assert_eq!(auto.face, ControlFace::Chip);
        assert_eq!(auto.row_columns, 5);
        for label in ["Loop", "Ring", "Connected", "Boundary", "Grow", "Shrink"] {
            let chip = planned.iter().find(|control| control.text == label).unwrap_or_else(|| panic!("missing {label}"));
            assert_eq!(chip.face, ControlFace::Chip);
            assert_eq!(chip.row_columns, 3);
            assert!(!chip.checked);
        }
        assert_eq!(planned.iter().find(|control| control.text == "Loop").unwrap().row_index, 0);
        assert_eq!(planned.iter().find(|control| control.text == "Boundary").unwrap().row_index, 0);
        assert!(!planned.iter().any(|control| control.text == "Show Grid"));
        assert!(!planned.iter().any(|control| {
            matches!(control.text.as_str(), "Extrude" | "Inset" | "Union" | "Subtract" | "Sketch" | "Edit Edges" | "Shell" | "Cancel" | "Apply" | "Move Edge" | "Extrude Edge" | "Split Edge" | "Subdivide" | "4×4")
        }));
        let mut face_model = build_solid(&selection, &world, &[], &[], SolidElement::Face(1), "Auto");
        assert_eq!(face_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "-X");
        assert_eq!(face_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_AREA).unwrap().display, "4.000 m²");
        assert_eq!(face_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_NORMAL).unwrap().display, "-X");
        assert!(face_model.section_titles().iter().any(|title| *title == "Model"));
        assert!(!face_model.section_titles().iter().any(|title| *title == "Face Tools"));
        let face_plan = plan(&face_model, &PlanOptions::editing());
        for label in ["Extrude", "Inset", "Bevel", "Subdivide", "4×4"] {
            assert!(face_plan.iter().any(|control| control.text == label), "missing {label}");
        }
        let extrude = face_plan.iter().find(|control| control.text == "Extrude").unwrap();
        assert_eq!(extrude.face, ControlFace::Tile);
        assert_eq!(extrude.row_columns, 2);
        assert!(!face_plan.iter().any(|control| matches!(control.text.as_str(), "Reset Shape" | "Duplicate" | "Mirror X" | "Move Edge")));
        let edge_model = build_solid(&selection, &world, &[], &[], SolidElement::Edge(12), "Edge");
        assert_eq!(edge_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "E:12");
        assert_eq!(edge_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_LENGTH).unwrap().display, "2.000 m");
        assert!(edge_model.section_titles().iter().any(|title| *title == "Model"));
        let edge_plan = plan(&edge_model, &PlanOptions::editing());
        for label in ["Move Edge", "Extrude Edge", "Split Edge", "Bevel", "Round", "Edge"] {
            assert!(edge_plan.iter().any(|control| control.text == label), "missing {label}");
        }
        assert!(!edge_plan.iter().any(|control| matches!(control.text.as_str(), "Extrude" | "Inset" | "Reset Shape" | "Subdivide" | "4×4")));
        let vertex_model = build_solid(&selection, &world, &[], &[], SolidElement::Vertex(3), "Vertex");
        assert_eq!(vertex_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "V:3");
        assert!(vertex_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_LENGTH).is_none());
        assert!(vertex_model.section_titles().iter().any(|title| *title == "Model"));
        let vertex_plan = plan(&vertex_model, &PlanOptions::editing());
        assert!(vertex_plan.iter().any(|control| control.text == "Move"));
        let vert = vertex_plan.iter().find(|control| control.text == "Vert").unwrap();
        assert!(vert.checked);
        assert!(!vertex_plan.iter().any(|control| matches!(control.text.as_str(), "Move Edge" | "Extrude" | "Bevel" | "Subdivide" | "4×4")));
        world.set_block_bevel(block, 0.9).unwrap();
        let featured = build(&selection, &world);
        assert!(plan(&featured, &PlanOptions::editing()).iter().any(|control| control.text == "Bevel  0.900 m"));
        set_selected_face_label(&mut face_model, "+X");
        assert_eq!(face_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "+X");
        assert!(plan(&model, &PlanOptions::editing()).iter().any(|control| control.text == "Reset Shape"));
        attach_modeling_session(
            &mut model,
            &ModelingView {
                title: "Bevel",
                face: "All edges".into(),
                amount_label: "Amount",
                amount: 0.6,
                minimum: 0.0,
                maximum: 0.9,
                show_amount: true,
                element_label: "Face",
                profile: None,
                resolution: None,
                range: None,
                resolved: None,
                readiness: None,
                note: None,
            },
        );
        assert!(model.section_titles().iter().any(|title| *title == "Active Tool"));
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_TOOL).unwrap().display, "Bevel");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "All edges");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_AMOUNT).unwrap().display, "0.600");
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_AMOUNT).unwrap().editable);
        let session_plan = plan(&model, &PlanOptions::editing());
        for label in ["Amount", "0.600", "Cancel", "Done", "Duplicate", "Align", "Auto", "Object", "Loop", "Ring", "Shrink"] {
            assert!(session_plan.iter().any(|control| control.text == label), "missing {label}");
        }
        assert!(!session_plan.iter().any(|control| control.text == "Apply"));
        let mut round_model = build(&selection, &world);
        attach_modeling_session(
            &mut round_model,
            &ModelingView {
                title: "Round",
                face: "1 Edge".into(),
                amount_label: "Radius",
                amount: 0.05,
                minimum: 0.001,
                maximum: 0.9,
                show_amount: true,
                element_label: "Selection",
                profile: Some("Circular"),
                resolution: Some("Automatic / Observation"),
                range: Some("0.001 – 1.250 m".into()),
                resolved: Some("2".into()),
                readiness: Some("Valid".into()),
                note: Some("3 picked edges resolve to 2 semantic edges.".into()),
            },
        );
        assert_eq!(round_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_TOOL).unwrap().display, "Round");
        assert_eq!(round_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "1 Edge");
        assert_eq!(round_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_RESOLVED).unwrap().display, "2");
        assert_eq!(round_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_STATUS).unwrap().display, "Valid");
        assert_eq!(round_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_ROUND_NOTE).unwrap().display, "3 picked edges resolve to 2 semantic edges.");
        assert_eq!(round_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_PROFILE).unwrap().display, "Circular");
        assert_eq!(round_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_RESOLUTION).unwrap().display, "Automatic / Observation");
        assert_eq!(round_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_RANGE).unwrap().display, "0.001 – 1.250 m");
        let round_plan = plan(&round_model, &PlanOptions::editing());
        assert!(round_plan.iter().any(|control| control.text == "Apply"));
        assert!(!round_plan.iter().any(|control| control.text == "Done"));
        assert!(!session_plan.iter().any(|control| {
            matches!(control.text.as_str(), "Reset Shape" | "Extrude" | "Inset" | "Bevel" | "Move Edge" | "Subdivide" | "4×4")
                && matches!(control.binding, InspectorBinding::Command { .. })
        }));
        let shaped = world.create_block(jarvig_core::Vec3::new(3.0, 1.0, -4.0), jarvig_core::BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        selection.replace(SelectionItem::entity(shaped).unwrap()).unwrap();
        world.subdivide_block_face(shaped, 3, 2, 2).unwrap();
        let stored = build_solid(&selection, &world, &[], &[], SolidElement::BodyFace(3), "Face");
        assert_eq!(stored.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "F:3");
        assert_eq!(stored.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_AREA).unwrap().display, "1.000 m²");
        assert_eq!(stored.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_NORMAL).unwrap().display, "+Y");
        assert!(stored.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACES).unwrap().display.parse::<u32>().unwrap() > 6);
        let grid = plan(&stored, &PlanOptions::editing());
        assert!(grid.iter().any(|control| control.text == "Show Grid" && matches!(control.binding, InspectorBinding::ShowGrid)));
        assert!(grid.iter().any(|control| control.text.contains("Subdivide")));
    }

    #[test]
    fn face_material_card_names_the_object_material_and_offers_make_unique() {
        let mut world = SceneWorld::bootstrap();
        let block = world.create_block(jarvig_core::Vec3::new(0.0, 1.0, -4.0), jarvig_core::BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let mut selection = SelectionService::default();
        selection.replace(SelectionItem::entity(block).unwrap()).unwrap();
        let object = build(&selection, &world);
        assert_eq!(object.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_SLOT).unwrap().display, "Object Material (Slot 0)");
        assert_eq!(object.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_NOTE).unwrap().display, SURFACE_OBJECT_NOTE);
        assert!(object.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_COUNT).is_none());
        assert!(object.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_ASSIGN).is_none());
        let object_plan = plan(&object, &PlanOptions::editing());
        assert!(object_plan.iter().any(|control| control.text == "New Slot"));
        assert!(object_plan.iter().any(|control| control.text == "Select Faces Using Material"));
        assert!(!object_plan.iter().any(|control| control.text == "Make Unique"));

        let record = world.authored_block(block).unwrap();
        let mut model = build(&selection, &world);
        set_surface_material(&mut model, &record, &[2]);
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_COUNT).unwrap().display, "1 Face");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_SLOT).unwrap().display, "Object Material (Slot 0)");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_NOTE).unwrap().display, SURFACE_SHARED_NOTE);
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_ASSIGN).unwrap().display, "Object Material");
        assert!(!selection_owns_material(&record, &[2], 0));
        let face_plan = plan(&model, &PlanOptions::editing());
        let unique = face_plan.iter().position(|control| matches!(control.binding, InspectorBinding::Command { command: InspectorCommand::MakeUnique, .. })).unwrap();
        let assign = face_plan.iter().position(|control| matches!(control.binding, InspectorBinding::Choice { field: SOLID_UI_SURFACE_ASSIGN, .. })).unwrap();
        let color = face_plan.iter().position(|control| matches!(control.binding, InspectorBinding::Text { field: SOLID_UI_COLOR_R, .. })).unwrap();
        let select = face_plan.iter().position(|control| matches!(control.binding, InspectorBinding::Command { command: InspectorCommand::SelectSlotFaces, .. })).unwrap();
        assert!(unique < assign && assign < color && color < select);
        assert!(!face_plan.iter().any(|control| control.text == "New Slot"));

        set_surface_material(&mut model, &record, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_COUNT).unwrap().display, "6 Faces");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_NOTE).unwrap().display, SURFACE_WHOLE_NOTE);
        assert!(selection_owns_material(&record, &[1, 2, 3, 4, 5, 6], 0));

        let mut painted = record.clone();
        painted.add_material_slot().unwrap();
        assert!(painted.set_material_factors(1, [1.0, 0.0, 0.0], 0.25, 0.8).unwrap());
        assert!(painted.assign_faces(&[1, 3, 5], 1, |_| Vec::new()).unwrap());
        set_surface_material(&mut model, &painted, &[1, 3, 5]);
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_COUNT).unwrap().display, "3 Faces");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_SLOT).unwrap().display, "Face Material 1 (Slot 1)");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_NOTE).unwrap().display, SURFACE_OWN_NOTE);
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_COLOR_R).unwrap().display, "1.000");
        assert!(selection_owns_material(&painted, &[1, 3, 5], 1));
        set_surface_material(&mut model, &painted, &[1]);
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_NOTE).unwrap().display, SURFACE_SHARED_NOTE);
        assert!(!selection_owns_material(&painted, &[1], 1));
        set_surface_material(&mut model, &painted, &[1, 2]);
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_SLOT).unwrap().display, "Mixed");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_NOTE).unwrap().display, SURFACE_MIXED_NOTE);
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_COLOR_R).is_none());
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_SURFACE_ASSIGN).unwrap().choices[0], "Mixed");
        assert!(plan(&model, &PlanOptions::editing()).iter().any(|control| control.text == "Make Unique"));
        assert!(!plan(&model, &PlanOptions::editing()).iter().any(|control| control.text == "Select Faces Using Material" || control.text == "New Slot"));
    }

    #[test]
    fn surface_group_card_names_upper_rim_after_the_material_controls() {
        let mut world = SceneWorld::bootstrap();
        let block = world.create_block(jarvig_core::Vec3::new(0.0, 1.0, -4.0), jarvig_core::BlockRecord::standard([2.0, 2.0, 2.0]).unwrap()).unwrap();
        let mut selection = SelectionService::default();
        selection.replace(SelectionItem::entity(block).unwrap()).unwrap();
        let plain = build(&selection, &world);
        assert_eq!(plain.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_GROUP_MEMBERSHIP).unwrap().display, "None");
        assert_eq!(plain.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_GROUP_PICK).unwrap().display, "None");
        assert!(plain.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_GROUP_NAME).unwrap().display.is_empty());

        let record = world.authored_block(block).unwrap();
        let mut model = build(&selection, &world);
        set_surface_material(&mut model, &record, &[3, 5]);
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_GROUP_MEMBERSHIP).unwrap().display, "None");
        let before = plan(&model, &PlanOptions::editing());
        let select = before.iter().position(|control| matches!(control.binding, InspectorBinding::Command { command: InspectorCommand::SelectSlotFaces, .. })).unwrap();
        let create = before.iter().position(|control| matches!(control.binding, InspectorBinding::Command { command: InspectorCommand::CreateSurfaceGroup, .. })).unwrap();
        assert!(select < create);
        for label in ["Create Group", "Rename Group", "Add To Group", "Remove From Group", "Select Group", "Delete Group"] {
            assert!(before.iter().any(|control| control.text == label), "missing {label}");
        }

        assert_eq!(world.create_block_surface_group(block, "Upper Rim", &[3, 5]).unwrap(), jarvig_core::AuthoringResult::Applied);
        let grouped = world.authored_block(block).unwrap();
        set_surface_material(&mut model, &grouped, &[3, 5]);
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_GROUP_MEMBERSHIP).unwrap().display, "Upper Rim");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_GROUP_NAME).unwrap().display, "Upper Rim");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_GROUP_PICK).unwrap().display, "Upper Rim");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_GROUP_PICK).unwrap().choices, vec!["Upper Rim".to_string()]);
        set_surface_material(&mut model, &grouped, &[]);
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_GROUP_MEMBERSHIP).unwrap().display, "None");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_GROUP_PICK).unwrap().display, "Upper Rim");
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_GROUP_NAME).unwrap().display.is_empty());

        let mut unresolved = grouped.clone();
        unresolved.surface_groups[0].members.push(jarvig_core::SurfaceMember { face: None, provenance: vec!["F:seed/1".into()] });
        set_surface_material(&mut model, &unresolved, &[3]);
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_GROUP_MEMBERSHIP).unwrap().display, "Upper Rim (unresolved)");
    }
}

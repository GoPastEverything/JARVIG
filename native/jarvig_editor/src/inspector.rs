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
    /// Drops the open modeling preview and restores the solid.
    CancelModeling,
    /// Writes one history entry for the open modeling preview.
    ApplyModeling,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlClass {
    Label,
    Edit,
    Check,
    Button,
    Combo,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InspectorBinding {
    Label,
    Text { type_id: TypeId, field: FieldId },
    Axis { type_id: TypeId, field: FieldId, axis: u8 },
    Check { type_id: TypeId, field: FieldId },
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanOptions {
    pub banner: Option<String>,
    pub advanced_open: bool,
    pub collapsed: Vec<String>,
    pub uniform_scale: bool,
    pub show_commands: bool,
    pub show_add: bool,
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
    /// Buttons whose label is not the static command name. Feature rows use this.
    pub feature_edits: Vec<(InspectorCommand, String)>,
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
    let (element_name, element_value, edge_length) = match element {
        SolidElement::Face(face) => ("Face", jarvig_core::face_name(face).to_string(), None),
        SolidElement::BodyFace(0) | SolidElement::Object => ("Face", "None".into(), None),
        SolidElement::BodyFace(face) => ("Face", format!("F:{face}"), None),
        SolidElement::Edge(0) => ("Edge", "None".into(), None),
        SolidElement::Edge(edge) => ("Edge", format!("E:{edge}"), record.display_body().and_then(|body| body.edge_length(edge))),
        SolidElement::Vertex(0) => ("Vertex", "None".into(), None),
        SolidElement::Vertex(vertex) => ("Vertex", format!("V:{vertex}"), None),
    };
    let mut selection_fields = vec![solid_note(SOLID_UI_FACE, element_name, element_value)];
    if let Some(length) = edge_length {
        selection_fields.push(solid_note(SOLID_UI_LENGTH, "Length", format!("{length:.3} m")));
    }
    let mode_edits = ["Auto", "Object", "Face", "Edge", "Vertex"]
        .into_iter()
        .zip([
            InspectorCommand::SelectionAuto,
            InspectorCommand::SelectionObject,
            InspectorCommand::SelectionFace,
            InspectorCommand::SelectionEdge,
            InspectorCommand::SelectionVertex,
        ])
        .map(|(name, command)| {
            let label = if name == mode_name { format!("{name}  ·") } else { name.to_string() };
            (command, label)
        })
        .collect();
    insert_section_after(
        sections,
        "Actor",
        InspectorSection {
            title: "Selection".into(),
            type_id: TYPE_PARAMETRIC_BLOCK,
            fields: selection_fields,
            commands: Vec::new(),
            feature_edits: mode_edits,
        },
    );
    let mut feature_edits = Vec::new();
    if record.bevel_m > 1.0e-9 {
        feature_edits.push((InspectorCommand::EditBevel, format!("Bevel  {:.3} m", record.bevel_m)));
    }
    for face in 0..6u8 {
        let inset = record.inset_m[face as usize];
        if inset > 1.0e-9 {
            feature_edits.push((
                InspectorCommand::EditInset(face),
                format!("Inset {}  {inset:.3} m", jarvig_core::face_name(face)),
            ));
        }
    }
    let mut feature_fields = vec![solid_note(
        SOLID_UI_BLOCK,
        "Block",
        format!("{:.3} × {:.3} × {:.3} m", record.size_m[0], record.size_m[1], record.size_m[2]),
    )];
    for (index, summary) in topology_log(record).into_iter().enumerate() {
        feature_fields.push(solid_note(FieldId(260 + index as u32), "Edit", summary));
    }
    insert_section_after(
        sections,
        "Dimensions",
        InspectorSection {
            title: "Features".into(),
            type_id: TYPE_PARAMETRIC_BLOCK,
            fields: feature_fields,
            commands: Vec::new(),
            feature_edits,
        },
    );
    let face_tools = vec![
        InspectorCommand::ExtrudeFace,
        InspectorCommand::InsetFace,
        InspectorCommand::Bevel,
        InspectorCommand::SubdivideFace,
    ];
    let edge_tools = vec![InspectorCommand::MoveEdge, InspectorCommand::ExtrudeEdge, InspectorCommand::SplitEdge];
    let (title, commands) = if mode_name == "Edge" || matches!(element, SolidElement::Edge(_)) {
        ("Edge Tools", edge_tools)
    } else if mode_name == "Vertex" || matches!(element, SolidElement::Vertex(_)) {
        ("Vertex Tools", vec![InspectorCommand::MoveVertex])
    } else if mode_name == "Face" || matches!(element, SolidElement::Face(_) | SolidElement::BodyFace(_)) {
        ("Face Tools", face_tools)
    } else {
        let mut commands = vec![InspectorCommand::Bevel, InspectorCommand::ResetShape];
        if capabilities.patternable {
            commands.extend([
                InspectorCommand::Duplicate,
                InspectorCommand::MirrorX,
                InspectorCommand::MirrorY,
                InspectorCommand::MirrorZ,
                InspectorCommand::AlignToGround,
                InspectorCommand::SnapTranslation,
            ]);
        }
        ("Object Tools", commands)
    };
    insert_section_after(
        sections,
        "Features",
        InspectorSection {
            title: title.into(),
            type_id: TYPE_PARAMETRIC_BLOCK,
            fields: Vec::new(),
            commands,
            feature_edits: Vec::new(),
        },
    );
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
}

/// Replaces the start buttons with Cancel, Apply, and the amount when the operation has one.
pub fn attach_modeling_session(model: &mut InspectorModel, view: &ModelingView) {
    let InspectorBody::Entity { sections } = &mut model.body else { return };
    for section in sections.iter_mut() {
        section.commands.retain(|command| {
            !matches!(
                command,
                InspectorCommand::ExtrudeFace
                    | InspectorCommand::InsetFace
                    | InspectorCommand::Bevel
                    | InspectorCommand::ResetShape
                    | InspectorCommand::MoveEdge
                    | InspectorCommand::ExtrudeEdge
                    | InspectorCommand::SplitEdge
                    | InspectorCommand::MoveVertex
                    | InspectorCommand::SubdivideFace
            )
        });
        section.feature_edits.clear();
        section.fields.retain(|field| field.field != SOLID_UI_FACE && !(260..268).contains(&field.field.0));
    }
    let mut fields = vec![solid_note(SOLID_UI_FACE, view.element_label, view.face.clone())];
    if view.show_amount {
        fields.push(solid_amount(view));
    }
    let section = InspectorSection {
        title: view.title.into(),
        type_id: TYPE_PARAMETRIC_BLOCK,
        fields,
        commands: vec![InspectorCommand::CancelModeling, InspectorCommand::ApplyModeling],
        feature_edits: Vec::new(),
    };
    let index = sections.iter().position(|existing| existing.title == "Features").unwrap_or(sections.len());
    sections.insert(index, section);
}

/// Last topology edits, as labels. Clicking one does not rebuild the solid.
fn topology_log(record: &jarvig_core::BlockRecord) -> Vec<String> {
    let rows: Vec<String> = record
        .history
        .iter()
        .filter_map(|op| match op {
            jarvig_core::BlockOp::MoveEdge { .. }
            | jarvig_core::BlockOp::ExtrudeEdge { .. }
            | jarvig_core::BlockOp::SplitEdge { .. }
            | jarvig_core::BlockOp::SubdivideFace { .. }
            | jarvig_core::BlockOp::MoveVertex { .. } => Some(op.summary()),
            _ => None,
        })
        .collect();
    let start = rows.len().saturating_sub(8);
    rows[start..].to_vec()
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

fn insert_section_after(sections: &mut Vec<InspectorSection>, after: &str, section: InspectorSection) {
    if sections.iter().any(|existing| existing.title == section.title) {
        return;
    }
    let index = sections.iter().position(|existing| existing.title == after).map(|index| index + 1).unwrap_or(sections.len());
    sections.insert(index, section);
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
                for field in &section.fields {
                    if field.field == FIELD_CAPTURE_STATE {
                        continue;
                    }
                    y = push_field(&mut items, field, y, options);
                }
                if options.show_commands {
                    for command in &section.commands {
                        if command_in_header(*command) {
                            continue;
                        }
                        items.push(control(
                            ControlClass::Button,
                            InspectorBinding::Command { command: *command, type_id: section.type_id },
                            command_label(*command).into(),
                            12,
                            y,
                        ));
                        y += 24;
                    }
                    for (command, label) in &section.feature_edits {
                        items.push(control(
                            ControlClass::Button,
                            InspectorBinding::Command { command: *command, type_id: section.type_id },
                            label.clone(),
                            12,
                            y,
                        ));
                        y += 24;
                    }
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
        InspectorCommand::CancelModeling => "Cancel",
        InspectorCommand::ApplyModeling => "Apply",
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
        InspectorCommand::MoveEdge => "Move Edge",
        InspectorCommand::ExtrudeEdge => "Extrude Edge",
        InspectorCommand::SplitEdge => "Split Edge",
        InspectorCommand::MoveVertex => "Move",
        InspectorCommand::SubdivideFace => "Subdivide",
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
            let mut row = control(ControlClass::Check, InspectorBinding::Check { type_id: field.type_id, field: field.field }, field.label.clone(), 12, y);
            row.checked = matches!(field.display.as_str(), "true" | "1");
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
        for title in ["Dimensions", "Features", "Selection", "Object Tools", "Collision", "Material"] {
            assert!(titles.iter().any(|existing| *existing == title), "missing {title} in {titles:?}");
        }
        assert!(!titles.iter().any(|existing| {
            matches!(*existing, "Shape" | "Modeling" | "Pattern" | "Placement" | "Face Tools")
        }));
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_ORIGIN).unwrap().display, "Center");
        assert!(!model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_ORIGIN).unwrap().editable);
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_SIZE_X).unwrap().editable);
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_BEVEL).is_none());
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_INSET_PX).is_none());
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_HISTORY).is_none());
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_BLOCK).unwrap().display, "2.000 × 2.000 × 2.000 m");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, jarvig_core::FIELD_BLOCK_COLLISION).unwrap().display, "Analytic box");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "None");
        let planned = plan(&model, &PlanOptions::editing());
        for label in ["Reset Shape", "Bevel", "Duplicate", "Mirror X", "Mirror Y", "Mirror Z", "Align", "Snap", "Auto  ·", "Object", "Face", "Edge", "Vertex"] {
            assert!(planned.iter().any(|control| control.text == label), "missing {label}");
        }
        assert!(!planned.iter().any(|control| {
            matches!(control.text.as_str(), "Extrude" | "Inset" | "Union" | "Subtract" | "Sketch" | "Edit Edges" | "Shell" | "Cancel" | "Apply" | "Move Edge" | "Extrude Edge" | "Split Edge" | "Subdivide")
        }));
        let face_model = build_solid(&selection, &world, &[], &[], SolidElement::Face(1), "Auto");
        assert_eq!(face_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "-X");
        assert!(face_model.section_titles().iter().any(|title| *title == "Face Tools"));
        let face_plan = plan(&face_model, &PlanOptions::editing());
        for label in ["Extrude", "Inset", "Bevel", "Subdivide"] {
            assert!(face_plan.iter().any(|control| control.text == label), "missing {label}");
        }
        assert!(!face_plan.iter().any(|control| matches!(control.text.as_str(), "Reset Shape" | "Duplicate" | "Mirror X" | "Move Edge")));
        let edge_model = build_solid(&selection, &world, &[], &[], SolidElement::Edge(12), "Edge");
        assert_eq!(edge_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "E:12");
        assert_eq!(edge_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_LENGTH).unwrap().display, "2.000 m");
        assert!(edge_model.section_titles().iter().any(|title| *title == "Edge Tools"));
        let edge_plan = plan(&edge_model, &PlanOptions::editing());
        for label in ["Move Edge", "Extrude Edge", "Split Edge", "Edge  ·"] {
            assert!(edge_plan.iter().any(|control| control.text == label), "missing {label}");
        }
        assert!(!edge_plan.iter().any(|control| matches!(control.text.as_str(), "Extrude" | "Inset" | "Reset Shape" | "Subdivide")));
        let vertex_model = build_solid(&selection, &world, &[], &[], SolidElement::Vertex(3), "Vertex");
        assert_eq!(vertex_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "V:3");
        assert!(vertex_model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_LENGTH).is_none());
        assert!(vertex_model.section_titles().iter().any(|title| *title == "Vertex Tools"));
        let vertex_plan = plan(&vertex_model, &PlanOptions::editing());
        assert!(vertex_plan.iter().any(|control| control.text == "Move"));
        assert!(vertex_plan.iter().any(|control| control.text == "Vertex  ·"));
        assert!(!vertex_plan.iter().any(|control| matches!(control.text.as_str(), "Move Edge" | "Extrude" | "Bevel" | "Subdivide")));
        world.set_block_bevel(block, 0.9).unwrap();
        let featured = build(&selection, &world);
        assert!(plan(&featured, &PlanOptions::editing()).iter().any(|control| control.text == "Bevel  0.900 m"));
        set_selected_face_label(&mut model, "+X");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "+X");
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
            },
        );
        assert!(model.section_titles().iter().any(|title| *title == "Bevel"));
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_FACE).unwrap().display, "All edges");
        assert_eq!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_AMOUNT).unwrap().display, "0.600");
        assert!(model.field(TYPE_PARAMETRIC_BLOCK, SOLID_UI_AMOUNT).unwrap().editable);
        let session_plan = plan(&model, &PlanOptions::editing());
        for label in ["Amount", "0.600", "Cancel", "Apply", "Duplicate", "Align"] {
            assert!(session_plan.iter().any(|control| control.text == label), "missing {label}");
        }
        assert!(!session_plan.iter().any(|control| matches!(control.text.as_str(), "Reset Shape" | "Extrude" | "Inset" | "Bevel" | "Move Edge" | "Subdivide")));
    }
}

//! Context menus and the View menu tree. Labels only. The commands already exist.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuPlace {
    OutlinerObject { locked: bool, hidden: bool },
    OutlinerFolder,
    OutlinerEmpty,
    ViewportObject,
    ViewportFace { bevel: bool, inset: bool },
    ViewportEdge,
    ViewportVertex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextAction {
    Rename,
    Duplicate,
    Delete,
    Focus,
    Hide,
    Show,
    Lock,
    Unlock,
    CreateFolder,
    DeleteFolder,
    CreateBlock,
    CreatePlane,
    Extrude,
    Inset,
    Bevel,
    Round,
    Subdivide,
    CreateSurfaceGroup,
    AddToGroup,
    RemoveFromGroup,
    AssignMaterial,
    CreateMaterial,
    SelectConnected,
    Boundary,
    Grow,
    Shrink,
    Split,
    ExtrudeEdge,
    Loop,
    Ring,
    MoveVertex,
    MoveToFolder,
}

impl ContextAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rename => "Rename",
            Self::Duplicate => "Duplicate",
            Self::Delete => "Delete",
            Self::Focus => "Focus",
            Self::Hide => "Hide in Editor",
            Self::Show => "Show in Editor",
            Self::Lock => "Lock",
            Self::Unlock => "Unlock",
            Self::CreateFolder => "Create Folder",
            Self::DeleteFolder => "Delete Folder",
            Self::CreateBlock => "Block",
            Self::CreatePlane => "Plane",
            Self::Extrude => "Extrude",
            Self::Inset => "Inset",
            Self::Bevel => "Bevel",
            Self::Round => "Round",
            Self::Subdivide => "Subdivide",
            Self::CreateSurfaceGroup => "Create Surface Group",
            Self::AddToGroup => "Add To Group",
            Self::RemoveFromGroup => "Remove From Group",
            Self::AssignMaterial => "Assign Material",
            Self::CreateMaterial => "Create Material",
            Self::SelectConnected => "Select Connected",
            Self::Boundary => "Boundary",
            Self::Grow => "Grow",
            Self::Shrink => "Shrink",
            Self::Split => "Split",
            Self::ExtrudeEdge => "Extrude Edge",
            Self::Loop => "Loop",
            Self::Ring => "Ring",
            Self::MoveVertex => "Move Vertex",
            Self::MoveToFolder => "Move To Folder",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuLine {
    Action(ContextAction),
    Separator,
}

pub fn menu_entries(place: MenuPlace) -> Vec<MenuLine> {
    use ContextAction::*;
    use MenuLine::{Action, Separator};
    match place {
        MenuPlace::OutlinerObject { locked, hidden } => vec![
            Action(Rename),
            Action(Duplicate),
            Action(Delete),
            Action(Focus),
            Action(if hidden { Show } else { Hide }),
            Action(if locked { Unlock } else { Lock }),
            Separator,
            Action(CreateFolder),
            Action(MoveToFolder),
        ],
        MenuPlace::OutlinerFolder => vec![Action(Rename), Action(DeleteFolder), Action(CreateFolder)],
        MenuPlace::OutlinerEmpty => vec![Action(CreateFolder), Separator, Action(CreateBlock), Action(CreatePlane)],
        MenuPlace::ViewportObject => vec![
            Action(Rename),
            Action(Duplicate),
            Action(Delete),
            Action(Focus),
            Action(Hide),
            Separator,
            Action(CreateMaterial),
            Action(AssignMaterial),
        ],
        MenuPlace::ViewportFace { bevel, inset } => {
            let mut lines = vec![Action(Extrude)];
            if inset {
                lines.push(Action(Inset));
            }
            if bevel {
                lines.push(Action(Bevel));
            }
            lines.extend([
                Action(Subdivide),
                Separator,
                Action(CreateSurfaceGroup),
                Action(AddToGroup),
                Action(RemoveFromGroup),
                Action(AssignMaterial),
                Separator,
                Action(SelectConnected),
                Action(Boundary),
                Action(Grow),
                Action(Shrink),
            ]);
            lines
        }
        MenuPlace::ViewportEdge => vec![
            Action(Bevel),
            Action(Round),
            Action(Split),
            Action(ExtrudeEdge),
            Separator,
            Action(Loop),
            Action(Ring),
            Action(SelectConnected),
        ],
        MenuPlace::ViewportVertex => vec![Action(MoveVertex), Action(SelectConnected)],
    }
}

pub fn menu_labels(place: MenuPlace) -> Vec<&'static str> {
    menu_entries(place)
        .into_iter()
        .filter_map(|line| match line {
            MenuLine::Action(action) => Some(action.label()),
            MenuLine::Separator => None,
        })
        .collect()
}

#[derive(Clone, Copy)]
pub enum ViewNode {
    Item(usize, &'static str),
    Group(&'static str, &'static [ViewNode]),
    Gap,
}

const PANELS: &[ViewNode] = &[
    ViewNode::Item(1301, "World Outliner"),
    ViewNode::Item(1302, "Inspector"),
    ViewNode::Item(1303, "Content Browser"),
    ViewNode::Item(1304, "Output Log"),
    ViewNode::Item(1305, "Reset Layout"),
    ViewNode::Gap,
    ViewNode::Item(1390, "Character Editor"),
    ViewNode::Item(1395, "Land Mode"),
    ViewNode::Item(1391, "Reset Pose"),
    ViewNode::Item(1392, "Show Joints"),
    ViewNode::Item(1394, "Show All Joints"),
    ViewNode::Item(1393, "Show Joint Limits"),
    ViewNode::Item(1396, "Mesh Parts"),
];

const GRID: &[ViewNode] = &[ViewNode::Item(1419, "Show Grid")];

const CAMERAS: &[ViewNode] = &[
    ViewNode::Item(1361, "Create Camera Actor"),
    ViewNode::Item(1362, "Set Selected as Startup Camera"),
    ViewNode::Item(1360, "Pilot Selected Camera"),
];

const QUALITY: &[ViewNode] = &[
    ViewNode::Item(1306, "Exposure +"),
    ViewNode::Item(1307, "Exposure -"),
    ViewNode::Item(1308, "Reset Exposure"),
    ViewNode::Gap,
    ViewNode::Item(1339, "Renderer Quality: Baseline"),
    ViewNode::Item(1340, "Renderer Quality: Enhanced"),
    ViewNode::Item(1341, "Renderer Quality: High"),
    ViewNode::Gap,
    ViewNode::Item(1342, "Presentation Dither"),
    ViewNode::Item(1343, "Presentation: Tone Map"),
    ViewNode::Item(1344, "Presentation: 8-bit Steps"),
    ViewNode::Item(1345, "Presentation: Before Tone Curve"),
];

const PIPELINE: &[ViewNode] = &[
    ViewNode::Item(1410, "Normal"),
    ViewNode::Item(1379, "Leaf Truth"),
    ViewNode::Item(1411, "Frustum Only"),
    ViewNode::Item(1412, "Hierarchy (no occlusion)"),
    ViewNode::Item(1413, "Hierarchy + Occlusion"),
];

const MESHLETS: &[ViewNode] = &[
    ViewNode::Item(1363, "Show Meshlet Colors"),
    ViewNode::Item(1364, "Draw From Meshlets"),
    ViewNode::Item(1365, "Frustum Cull Meshlets"),
    ViewNode::Item(1366, "Occlusion Cull Meshlets"),
    ViewNode::Item(1367, "Freeze Meshlet Visibility"),
    ViewNode::Item(1368, "Highlight Cluster Under Cursor"),
    ViewNode::Item(1369, "Cluster Hierarchy"),
    ViewNode::Gap,
    ViewNode::Item(1372, "Hierarchy Error 0.5 px"),
    ViewNode::Item(1373, "Hierarchy Error 1 px"),
    ViewNode::Item(1374, "Hierarchy Error 2 px"),
    ViewNode::Item(1375, "Hierarchy Error 4 px"),
    ViewNode::Gap,
    ViewNode::Group("Pipeline", PIPELINE),
];

const VISUALIZATION: &[ViewNode] = &[
    ViewNode::Item(1416, "None"),
    ViewNode::Item(1387, "Cut Reasons"),
    ViewNode::Item(1414, "Hierarchy Levels"),
    ViewNode::Item(1382, "Einstein Active Detail"),
    ViewNode::Item(1383, "Einstein Eligible Surfaces"),
    ViewNode::Item(1384, "Einstein LOD / Error"),
    ViewNode::Item(1385, "Einstein State"),
    ViewNode::Item(1386, "Einstein Reject Reason"),
    ViewNode::Item(1376, "Einstein Detail Debug"),
];

const MICROGEOMETRY: &[ViewNode] = &[
    ViewNode::Item(1377, "Microgeometry Auto"),
    ViewNode::Item(1380, "Microgeometry On"),
    ViewNode::Item(1381, "Microgeometry Off"),
    ViewNode::Item(1378, "Microtriangle Colors"),
    ViewNode::Gap,
    ViewNode::Group("Visualization", VISUALIZATION),
];

const GEOMETRY_TRUTH: &[ViewNode] = &[
    ViewNode::Item(1420, "Intent Shadow"),
    ViewNode::Item(1418, "Compare Against Leaf Truth"),
];

const RENDERING: &[ViewNode] = &[
    ViewNode::Group("Quality", QUALITY),
    ViewNode::Group("Meshlets", MESHLETS),
    ViewNode::Group("Microgeometry", MICROGEOMETRY),
    ViewNode::Group("Geometry Truth", GEOMETRY_TRUTH),
];

const LIGHTING: &[ViewNode] = &[
    ViewNode::Item(1309, "Environment Light"),
    ViewNode::Item(1327, "Recapture Reflection Probes"),
    ViewNode::Gap,
    ViewNode::Item(1328, "Probe Update: Static"),
    ViewNode::Item(1329, "Probe Update: On Demand"),
    ViewNode::Item(1330, "Probe Update: On Transform"),
    ViewNode::Item(1331, "Probe Update: On Lighting"),
    ViewNode::Item(1336, "Probe Update: Time Sliced"),
    ViewNode::Gap,
    ViewNode::Item(1332, "Probe Resolution 32"),
    ViewNode::Item(1333, "Probe Resolution 64"),
    ViewNode::Item(1334, "Probe Resolution 128"),
    ViewNode::Item(1335, "Probe Resolution 256"),
    ViewNode::Gap,
    ViewNode::Item(1310, "Full Lighting"),
    ViewNode::Item(1311, "Direct Only"),
    ViewNode::Item(1312, "Environment Diffuse Only"),
    ViewNode::Item(1313, "Environment Specular Only"),
    ViewNode::Item(1314, "Local Probe Specular Only"),
    ViewNode::Item(1315, "Emissive Only"),
    ViewNode::Gap,
    ViewNode::Item(1316, "Directional Light"),
    ViewNode::Item(1317, "Point Light"),
    ViewNode::Item(1318, "Spot Light"),
    ViewNode::Gap,
    ViewNode::Item(1319, "Global Environment"),
    ViewNode::Item(1320, "Reflection Probe"),
    ViewNode::Gap,
    ViewNode::Item(1321, "Directional Only"),
    ViewNode::Item(1322, "Point Only"),
    ViewNode::Item(1323, "Spot Only"),
    ViewNode::Item(1324, "No Shadows"),
    ViewNode::Item(1325, "Indirect Diffuse Only"),
    ViewNode::Item(1326, "Direct Unshadowed"),
    ViewNode::Gap,
    ViewNode::Item(1337, "Shadow Cascades"),
    ViewNode::Item(1338, "Contact Shadows"),
    ViewNode::Gap,
    ViewNode::Item(1346, "Material: Full"),
    ViewNode::Item(1347, "Material: Base Color"),
    ViewNode::Item(1348, "Material: Normal"),
    ViewNode::Item(1349, "Material: Roughness"),
    ViewNode::Item(1350, "Material: AO"),
    ViewNode::Item(1351, "Material: Metallic"),
];

const DIAGNOSTICS: &[ViewNode] = &[
    ViewNode::Item(1417, "Freeze Diagnostic Frame"),
    ViewNode::Item(1415, "Reset Rendering Debug"),
    ViewNode::Item(1370, "Background Jobs"),
    ViewNode::Item(1371, "Cancel Background Job"),
];

pub const VIEW_MENU: &[ViewNode] = &[
    ViewNode::Group("Panels", PANELS),
    ViewNode::Group("Grid & Guides", GRID),
    ViewNode::Group("Cameras", CAMERAS),
    ViewNode::Group("Rendering", RENDERING),
    ViewNode::Group("Lighting", LIGHTING),
    ViewNode::Group("Diagnostics", DIAGNOSTICS),
];

pub fn view_command_ids() -> Vec<usize> {
    let mut ids = Vec::new();
    collect_view_ids(VIEW_MENU, &mut ids);
    ids
}

fn collect_view_ids(nodes: &[ViewNode], ids: &mut Vec<usize>) {
    for node in nodes {
        match node {
            ViewNode::Item(id, _) => ids.push(*id),
            ViewNode::Group(_, children) => collect_view_ids(children, ids),
            ViewNode::Gap => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menus_name_existing_commands_and_skip_render_triangles() {
        let object = menu_labels(MenuPlace::ViewportObject);
        assert!(object.contains(&"Rename"));
        assert!(object.contains(&"Hide in Editor"));
        assert!(object.contains(&"Create Material"));
        let faces = menu_labels(MenuPlace::ViewportFace { bevel: true, inset: true });
        for label in ["Extrude", "Inset", "Bevel", "Subdivide", "Create Surface Group", "Add To Group", "Remove From Group", "Assign Material", "Select Connected", "Boundary", "Grow", "Shrink"] {
            assert!(faces.contains(&label), "{label}");
        }
        let edited = menu_labels(MenuPlace::ViewportFace { bevel: false, inset: false });
        assert!(!edited.contains(&"Bevel"));
        assert!(!edited.contains(&"Inset"));
        assert!(edited.contains(&"Extrude"));
        let edges = menu_labels(MenuPlace::ViewportEdge);
        for label in ["Bevel", "Round", "Split", "Extrude Edge", "Loop", "Ring", "Select Connected"] {
            assert!(edges.contains(&label), "{label}");
        }
        let vertices = menu_labels(MenuPlace::ViewportVertex);
        assert_eq!(vertices, vec!["Move Vertex", "Select Connected"]);
        assert!(!vertices.iter().any(|label| label.contains("Bevel")));
        let outliner = menu_labels(MenuPlace::OutlinerObject { locked: false, hidden: false });
        for label in ["Rename", "Duplicate", "Delete", "Focus", "Hide in Editor", "Lock", "Create Folder", "Move To Folder"] {
            assert!(outliner.contains(&label), "{label}");
        }
        assert!(menu_labels(MenuPlace::OutlinerObject { locked: true, hidden: true }).contains(&"Show in Editor"));
        let empty = menu_labels(MenuPlace::OutlinerEmpty);
        assert!(empty.contains(&"Create Folder"));
        assert!(empty.contains(&"Block"));
        assert!(empty.contains(&"Plane"));
        let joined = menu_labels(MenuPlace::ViewportFace { bevel: true, inset: true }).join(" ");
        assert!(!joined.to_ascii_lowercase().contains("meshlet"));
        assert!(!joined.to_ascii_lowercase().contains("triangle"));
    }

    #[test]
    fn the_view_menu_groups_every_command_once() {
        let ids = view_command_ids();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len());
        assert!(ids.contains(&1301));
        assert!(ids.contains(&1419));
        assert!(ids.contains(&1363));
        assert!(ids.contains(&1376));
        assert!(ids.contains(&1420));
        assert!(ids.contains(&1310));
        assert!(ids.contains(&1417));
        assert_eq!(ids.len(), 97);
    }
}

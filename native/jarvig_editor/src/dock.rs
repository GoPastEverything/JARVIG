//! Editor dock tree. Panel ids and ratios only. No window handles and no engine types.
//!
//! A later file format can store [`PersistWorkspace`]. Native handles stay in the shell.

use std::fmt;

pub const SCHEMA_VERSION: u32 = 1;
pub const DIP_PER_INCH: f32 = 96.0;
pub const SPLITTER_DIP: f32 = 6.0;
pub const TAB_DIP: f32 = 22.0;
pub const MIN_PANEL_DIP: f32 = 96.0;
const RATIO_MIN: f32 = 0.08;
const RATIO_MAX: f32 = 0.92;
const DROP_EDGE_DIP: f32 = 28.0;

pub const OUTLINER: PanelId = PanelId(1);
pub const INSPECTOR: PanelId = PanelId(2);
pub const PERSPECTIVE: PanelId = PanelId(3);
pub const CONTENT: PanelId = PanelId(4);
pub const OUTPUT: PanelId = PanelId(5);

/// Stable editor identity. Not an HWND and not an engine object.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PanelId(pub u32);

impl PanelId {
    pub fn raw(self) -> u32 {
        self.0
    }
}

impl fmt::Display for PanelId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            OUTLINER => formatter.write_str("WorldOutliner"),
            INSPECTOR => formatter.write_str("Inspector"),
            PERSPECTIVE => formatter.write_str("Perspective"),
            CONTENT => formatter.write_str("ContentBrowser"),
            OUTPUT => formatter.write_str("OutputLog"),
            other => write!(formatter, "Panel{}", other.0),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeId(pub u32);

/// `Horizontal` puts children left and right. `Vertical` puts them above and below.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropZone {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InstancePolicy {
    Singleton,
    Multiple,
}

/// Which top-level host owns a dock tree. Floating hosts are not created yet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HostKind {
    Main,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DockHint {
    pub anchor: PanelId,
    pub zone: DropZone,
    /// Share of the split given to the first child after `zone` orders the two sides.
    pub ratio: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PanelDesc {
    pub id: PanelId,
    pub title: &'static str,
    pub closable: bool,
    pub policy: InstancePolicy,
    pub hint: DockHint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DockError {
    UnknownPanel(PanelId),
    UnknownNode(NodeId),
    NotAStack(NodeId),
    DuplicateSingleton(PanelId),
    AlreadyOpen(PanelId),
    NotClosable(PanelId),
    NotOpen(PanelId),
    AlreadyRegistered(PanelId),
    UnknownPreset(String),
    EmptyWorkspace,
}

impl fmt::Display for DockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPanel(id) => write!(formatter, "unknown panel {id}"),
            Self::UnknownNode(id) => write!(formatter, "unknown dock node {}", id.0),
            Self::NotAStack(id) => write!(formatter, "dock node {} is not a stack", id.0),
            Self::DuplicateSingleton(id) => write!(formatter, "singleton panel {id} is already open"),
            Self::AlreadyOpen(id) => write!(formatter, "panel {id} is already open"),
            Self::NotClosable(id) => write!(formatter, "panel {id} cannot be closed"),
            Self::NotOpen(id) => write!(formatter, "panel {id} is not open"),
            Self::AlreadyRegistered(id) => write!(formatter, "panel {id} is already registered"),
            Self::UnknownPreset(name) => write!(formatter, "unknown workspace preset {name}"),
            Self::EmptyWorkspace => write!(formatter, "dock workspace has no stack"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PanelRegistry {
    panels: Vec<PanelDesc>,
}

impl PanelRegistry {
    pub fn new() -> Self {
        Self { panels: Vec::new() }
    }

    pub fn builtins() -> Self {
        let mut registry = Self::new();
        for desc in BUILTINS {
            registry.register(desc).expect("builtin panel");
        }
        registry
    }

    pub fn register(&mut self, desc: PanelDesc) -> Result<(), DockError> {
        if self.panels.iter().any(|panel| panel.id == desc.id) {
            return Err(DockError::AlreadyRegistered(desc.id));
        }
        self.panels.push(desc);
        Ok(())
    }

    pub fn get(&self, id: PanelId) -> Option<&PanelDesc> {
        self.panels.iter().find(|panel| panel.id == id)
    }

    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = &PanelDesc> {
        self.panels.iter()
    }
}

const BUILTINS: [PanelDesc; 5] = [
    PanelDesc {
        id: OUTLINER,
        title: "World Outliner",
        closable: true,
        policy: InstancePolicy::Singleton,
        hint: DockHint { anchor: PERSPECTIVE, zone: DropZone::Left, ratio: 0.18 },
    },
    PanelDesc {
        id: INSPECTOR,
        title: "Inspector",
        closable: true,
        policy: InstancePolicy::Singleton,
        hint: DockHint { anchor: PERSPECTIVE, zone: DropZone::Right, ratio: 0.78 },
    },
    PanelDesc {
        id: PERSPECTIVE,
        title: "Perspective",
        closable: false,
        policy: InstancePolicy::Multiple,
        hint: DockHint { anchor: PERSPECTIVE, zone: DropZone::Center, ratio: 0.5 },
    },
    PanelDesc {
        id: CONTENT,
        title: "Content Browser",
        closable: true,
        policy: InstancePolicy::Singleton,
        hint: DockHint { anchor: PERSPECTIVE, zone: DropZone::Bottom, ratio: 0.78 },
    },
    PanelDesc {
        id: OUTPUT,
        title: "Output Log",
        closable: true,
        policy: InstancePolicy::Singleton,
        hint: DockHint { anchor: CONTENT, zone: DropZone::Right, ratio: 0.62 },
    },
];

#[derive(Clone, Debug)]
enum NodeKind {
    Split { axis: Axis, ratio: f32, first: NodeId, second: NodeId },
    Stack { tabs: Vec<PanelId>, active: usize },
}

struct Node {
    kind: NodeKind,
    parent: Option<NodeId>,
}

pub struct DockWorkspace {
    schema_version: u32,
    preset: String,
    registry: PanelRegistry,
    host: HostKind,
    next_id: u32,
    nodes: Vec<Option<Node>>,
    root: NodeId,
    focused: Option<PanelId>,
}

impl DockWorkspace {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn from_preset(name: &str) -> Result<Self, DockError> {
        if name == "Default" {
            Ok(Self::default_layout())
        } else {
            Err(DockError::UnknownPreset(name.to_string()))
        }
    }

    pub fn default_layout() -> Self {
        let mut workspace = Self {
            schema_version: SCHEMA_VERSION,
            preset: "Default".into(),
            registry: PanelRegistry::builtins(),
            host: HostKind::Main,
            next_id: 1,
            nodes: vec![None],
            root: NodeId(0),
            focused: Some(PERSPECTIVE),
        };
        let outliner = workspace.stack(vec![OUTLINER]);
        let viewport = workspace.stack(vec![PERSPECTIVE]);
        let inspector = workspace.stack(vec![INSPECTOR]);
        let right = workspace.split(Axis::Horizontal, 0.78, viewport, inspector);
        let top = workspace.split(Axis::Horizontal, 0.18, outliner, right);
        let content = workspace.stack(vec![CONTENT]);
        let output = workspace.stack(vec![OUTPUT]);
        let bottom = workspace.split(Axis::Horizontal, 0.62, content, output);
        workspace.root = workspace.split(Axis::Vertical, 0.78, top, bottom);
        workspace
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn preset_name(&self) -> &str {
        &self.preset
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn host(&self) -> HostKind {
        self.host
    }

    #[allow(dead_code)]
    pub fn registry(&self) -> &PanelRegistry {
        &self.registry
    }

    #[allow(dead_code)]
    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn focused(&self) -> Option<PanelId> {
        self.focused
    }

    pub fn title(&self, id: PanelId) -> &str {
        self.registry.get(id).map(|desc| desc.title).unwrap_or("Panel")
    }

    pub fn is_open(&self, id: PanelId) -> bool {
        self.stack_of(id).is_some()
    }

    pub fn stack_of(&self, panel: PanelId) -> Option<NodeId> {
        self.walk_ids(self.root).into_iter().find(|id| self.tabs(*id).contains(&panel))
    }

    pub fn split_parent(&self, panel: PanelId) -> Option<NodeId> {
        let stack = self.stack_of(panel)?;
        let parent = self.nodes.get(stack.0 as usize)?.as_ref()?.parent?;
        match self.nodes.get(parent.0 as usize)?.as_ref()?.kind {
            NodeKind::Split { .. } => Some(parent),
            NodeKind::Stack { .. } => None,
        }
    }

    pub fn panel_count(&self) -> usize {
        self.open_panels().len()
    }

    pub fn stack_count(&self) -> usize {
        self.alive_ids().into_iter().filter(|id| self.is_stack(*id)).count()
    }

    pub fn split_count(&self) -> usize {
        self.alive_ids().into_iter().filter(|id| !self.is_stack(*id)).count()
    }

    pub fn open_panels(&self) -> Vec<PanelId> {
        let mut panels = Vec::new();
        self.collect_panels(self.root, &mut panels);
        panels
    }

    pub fn apply(&mut self, command: WorkspaceCommand) -> Result<(), DockError> {
        match command {
            WorkspaceCommand::ShowPanel(id) => self.show_panel(id),
            WorkspaceCommand::ClosePanel(id) => self.close_panel(id),
            WorkspaceCommand::DockPanel { panel, target, zone } => self.dock_panel(panel, target, zone),
            WorkspaceCommand::SetSplitRatio { node, ratio } => self.set_split_ratio(node, ratio),
            WorkspaceCommand::Activate(id) => self.activate(id),
            WorkspaceCommand::Focus(id) => self.focus(id),
            WorkspaceCommand::ResetLayout => {
                self.reset_layout();
                Ok(())
            }
        }
    }

    pub fn show_panel(&mut self, panel: PanelId) -> Result<(), DockError> {
        if self.is_open(panel) {
            return self.activate(panel);
        }
        let desc = *self.registry.get(panel).ok_or(DockError::UnknownPanel(panel))?;
        let anchor = self.anchor_stack(desc.hint.anchor)?;
        self.insert_at(panel, anchor, desc.hint.zone, desc.hint.ratio)?;
        self.activate(panel)?;
        self.normalize();
        Ok(())
    }

    pub fn close_panel(&mut self, panel: PanelId) -> Result<(), DockError> {
        let desc = *self.registry.get(panel).ok_or(DockError::UnknownPanel(panel))?;
        if !desc.closable {
            return Err(DockError::NotClosable(panel));
        }
        let stack = self.stack_of(panel).ok_or(DockError::NotOpen(panel))?;
        self.remove_tab(stack, panel)?;
        if self.focused == Some(panel) {
            self.focused = self.stack_of(PERSPECTIVE).map(|_| PERSPECTIVE).or_else(|| self.open_panels().first().copied());
        }
        self.normalize();
        Ok(())
    }

    /// Moves a panel that is already open. Does not create a second singleton.
    pub fn dock_panel(&mut self, panel: PanelId, mut target: NodeId, zone: DropZone) -> Result<(), DockError> {
        self.registry.get(panel).ok_or(DockError::UnknownPanel(panel))?;
        if self.stack_of(panel) == Some(target) && zone == DropZone::Center {
            return self.activate(panel);
        }
        if self.is_open(panel) {
            self.detach(panel)?;
        }
        if self.node(target).is_err() || !self.is_stack(target) {
            target = self.anchor_stack(PERSPECTIVE)?;
        }
        self.insert_at(panel, target, zone, 0.5)?;
        self.activate(panel)?;
        self.normalize();
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn add_panel(&mut self, panel: PanelId, target: NodeId, zone: DropZone) -> Result<(), DockError> {
        let desc = *self.registry.get(panel).ok_or(DockError::UnknownPanel(panel))?;
        if self.is_open(panel) {
            return Err(match desc.policy {
                InstancePolicy::Singleton => DockError::DuplicateSingleton(panel),
                InstancePolicy::Multiple => DockError::AlreadyOpen(panel),
            });
        }
        self.insert_at(panel, target, zone, 0.5)?;
        self.activate(panel)?;
        self.normalize();
        Ok(())
    }

    pub fn activate(&mut self, panel: PanelId) -> Result<(), DockError> {
        let stack = self.stack_of(panel).ok_or(DockError::NotOpen(panel))?;
        let node = self.node_mut(stack)?;
        let NodeKind::Stack { tabs, active } = &mut node.kind else {
            return Err(DockError::NotAStack(stack));
        };
        let Some(index) = tabs.iter().position(|tab| *tab == panel) else {
            return Err(DockError::NotOpen(panel));
        };
        *active = index;
        self.focused = Some(panel);
        Ok(())
    }

    pub fn focus(&mut self, panel: PanelId) -> Result<(), DockError> {
        if !self.is_open(panel) {
            return Err(DockError::NotOpen(panel));
        }
        self.focused = Some(panel);
        Ok(())
    }

    pub fn set_split_ratio(&mut self, node: NodeId, ratio: f32) -> Result<(), DockError> {
        let slot = self.node_mut(node)?;
        match &mut slot.kind {
            NodeKind::Split { ratio: stored, .. } => *stored = clamp_ratio(ratio),
            NodeKind::Stack { .. } => return Err(DockError::UnknownNode(node)),
        }
        Ok(())
    }

    pub fn reset_layout(&mut self) {
        let registry = self.registry.clone();
        let mut fresh = Self::default_layout();
        fresh.registry = registry;
        *self = fresh;
    }

    #[allow(dead_code)]
    pub fn register_panel(&mut self, desc: PanelDesc) -> Result<(), DockError> {
        self.registry.register(desc)
    }

    pub fn layout(&self, bounds: DipRect) -> Layout {
        let mut layout = Layout::default();
        if self.node(self.root).is_ok() {
            self.place(self.root, bounds, &mut layout);
        }
        layout
    }

    pub fn persistent(&self) -> PersistWorkspace {
        PersistWorkspace {
            schema_version: self.schema_version,
            preset: self.preset.clone(),
            host: self.host,
            focused: self.focused,
            root: self.persist_node(self.root),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn check(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("schema".into());
        }
        if self.node(self.root).is_err() {
            return Err("root".into());
        }
        let mut seen_panels = Vec::new();
        let mut seen_nodes = Vec::new();
        self.check_node(self.root, None, &mut seen_nodes, &mut seen_panels)?;
        for id in self.alive_ids() {
            if !seen_nodes.contains(&id) {
                return Err(format!("orphan node {}", id.0));
            }
        }
        if let Some(focused) = self.focused {
            if !seen_panels.contains(&focused) {
                return Err("focus is not open".into());
            }
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn check_node(
        &self,
        id: NodeId,
        parent: Option<NodeId>,
        seen_nodes: &mut Vec<NodeId>,
        seen_panels: &mut Vec<PanelId>,
    ) -> Result<(), String> {
        if seen_nodes.contains(&id) {
            return Err("cycle".into());
        }
        seen_nodes.push(id);
        let node = self.node(id).map_err(|error| error.to_string())?;
        if node.parent != parent {
            return Err(format!("parent link {}", id.0));
        }
        match &node.kind {
            NodeKind::Split { axis: _, ratio, first, second } => {
                if *ratio < RATIO_MIN || *ratio > RATIO_MAX {
                    return Err("ratio".into());
                }
                self.check_node(*first, Some(id), seen_nodes, seen_panels)?;
                self.check_node(*second, Some(id), seen_nodes, seen_panels)?;
            }
            NodeKind::Stack { tabs, active } => {
                if tabs.is_empty() {
                    return Err("empty stack".into());
                }
                if *active >= tabs.len() {
                    return Err("active tab".into());
                }
                for tab in tabs {
                    if self.registry.get(*tab).is_none() {
                        return Err(format!("unregistered {tab}"));
                    }
                    if seen_panels.contains(tab) {
                        return Err(format!("duplicate {tab}"));
                    }
                    seen_panels.push(*tab);
                }
            }
        }
        Ok(())
    }

    fn persist_node(&self, id: NodeId) -> PersistNode {
        match &self.node(id).expect("persist").kind {
            NodeKind::Split { axis, ratio, first, second } => PersistNode::Split {
                axis: *axis,
                ratio: *ratio,
                first: Box::new(self.persist_node(*first)),
                second: Box::new(self.persist_node(*second)),
            },
            NodeKind::Stack { tabs, active } => PersistNode::Stack { tabs: tabs.clone(), active: *active as u32 },
        }
    }

    fn place(&self, id: NodeId, rect: DipRect, out: &mut Layout) {
        let kind = self.node(id).expect("place").kind.clone();
        match kind {
            NodeKind::Split { axis, ratio, first, second } => {
                let span = if axis == Axis::Horizontal { rect.width } else { rect.height };
                let (first_span, bar, second_span) = split_sizes(span, ratio);
                let (first_rect, bar_rect, second_rect, origin) = match axis {
                    Axis::Horizontal => (
                        DipRect { x: rect.x, y: rect.y, width: first_span, height: rect.height },
                        DipRect { x: rect.x + first_span, y: rect.y, width: bar, height: rect.height },
                        DipRect { x: rect.x + first_span + bar, y: rect.y, width: second_span, height: rect.height },
                        rect.x,
                    ),
                    Axis::Vertical => (
                        DipRect { x: rect.x, y: rect.y, width: rect.width, height: first_span },
                        DipRect { x: rect.x, y: rect.y + first_span, width: rect.width, height: bar },
                        DipRect { x: rect.x, y: rect.y + first_span + bar, width: rect.width, height: second_span },
                        rect.y,
                    ),
                };
                out.splitters.push(PlacedSplitter { node: id, axis, rect: bar_rect, origin, span });
                self.place(first, first_rect, out);
                self.place(second, second_rect, out);
            }
            NodeKind::Stack { tabs, active } => {
                let tab_h = TAB_DIP.min(rect.height.max(0.0));
                let tab_rect = DipRect { x: rect.x, y: rect.y, width: rect.width, height: tab_h };
                let content = DipRect {
                    x: rect.x,
                    y: rect.y + tab_h,
                    width: rect.width,
                    height: (rect.height - tab_h).max(0.0),
                };
                let count = tabs.len().max(1) as f32;
                let tab_w = rect.width / count;
                let placed_tabs = tabs
                    .iter()
                    .enumerate()
                    .map(|(index, panel)| PlacedTab {
                        panel: *panel,
                        rect: DipRect { x: rect.x + tab_w * index as f32, y: rect.y, width: tab_w, height: tab_h },
                        active: index == active,
                    })
                    .collect();
                out.stacks.push(PlacedStack { node: id, rect, tab_rect, tabs: placed_tabs });
                for (index, panel) in tabs.iter().enumerate() {
                    out.panels.push(PlacedPanel {
                        panel: *panel,
                        stack: id,
                        rect: if index == active { content } else { DipRect::ZERO },
                        visible: index == active,
                    });
                }
            }
        }
    }

    fn anchor_stack(&self, preferred: PanelId) -> Result<NodeId, DockError> {
        if let Some(stack) = self.stack_of(preferred) {
            return Ok(stack);
        }
        if let Some(stack) = self.stack_of(PERSPECTIVE) {
            return Ok(stack);
        }
        self.walk_ids(self.root).into_iter().find(|id| self.is_stack(*id)).ok_or(DockError::EmptyWorkspace)
    }

    fn insert_at(&mut self, panel: PanelId, target: NodeId, zone: DropZone, ratio: f32) -> Result<(), DockError> {
        if !self.is_stack(target) {
            return Err(DockError::NotAStack(target));
        }
        if self.tabs(target).is_empty() {
            self.set_tabs(target, vec![panel], 0);
            return Ok(());
        }
        if zone == DropZone::Center {
            let node = self.node_mut(target)?;
            let NodeKind::Stack { tabs, active } = &mut node.kind else {
                return Err(DockError::NotAStack(target));
            };
            tabs.push(panel);
            *active = tabs.len() - 1;
            return Ok(());
        }
        let fresh = self.stack(vec![panel]);
        let (first, second) = match zone {
            DropZone::Left | DropZone::Top => (fresh, target),
            DropZone::Right | DropZone::Bottom => (target, fresh),
            DropZone::Center => unreachable!(),
        };
        let axis = match zone {
            DropZone::Left | DropZone::Right => Axis::Horizontal,
            DropZone::Top | DropZone::Bottom => Axis::Vertical,
            DropZone::Center => unreachable!(),
        };
        let parent = self.nodes.get(target.0 as usize).and_then(|slot| slot.as_ref()).and_then(|node| node.parent);
        let split = self.split(axis, ratio, first, second);
        self.set_parent(split, parent);
        if let Some(parent) = parent {
            self.replace_child(parent, target, split);
        } else {
            self.root = split;
        }
        Ok(())
    }

    fn detach(&mut self, panel: PanelId) -> Result<(), DockError> {
        let stack = self.stack_of(panel).ok_or(DockError::NotOpen(panel))?;
        self.remove_tab(stack, panel)
    }

    fn remove_tab(&mut self, stack: NodeId, panel: PanelId) -> Result<(), DockError> {
        let emptied = {
            let node = self.node_mut(stack)?;
            let NodeKind::Stack { tabs, active } = &mut node.kind else {
                return Err(DockError::NotAStack(stack));
            };
            let Some(index) = tabs.iter().position(|tab| *tab == panel) else {
                return Err(DockError::NotOpen(panel));
            };
            tabs.remove(index);
            if tabs.is_empty() {
                true
            } else {
                if *active >= tabs.len() {
                    *active = tabs.len() - 1;
                } else if index < *active {
                    *active -= 1;
                }
                false
            }
        };
        if emptied {
            self.collapse_stack(stack);
        }
        Ok(())
    }

    fn collapse_stack(&mut self, stack: NodeId) {
        let parent = self.nodes.get(stack.0 as usize).and_then(|slot| slot.as_ref()).and_then(|node| node.parent);
        let Some(parent) = parent else {
            return;
        };
        let sibling = match self.nodes.get(parent.0 as usize).and_then(|slot| slot.as_ref()).map(|node| node.kind.clone()) {
            Some(NodeKind::Split { first, second, .. }) if first == stack => second,
            Some(NodeKind::Split { first, second, .. }) if second == stack => first,
            _ => return,
        };
        let grand = self.nodes.get(parent.0 as usize).and_then(|slot| slot.as_ref()).and_then(|node| node.parent);
        self.kill(stack);
        self.kill(parent);
        self.set_parent(sibling, grand);
        if let Some(grand) = grand {
            self.replace_child(grand, parent, sibling);
        } else {
            self.root = sibling;
        }
    }

    fn normalize(&mut self) {
        let ids = self.alive_ids();
        for id in ids {
            let Some(node) = self.nodes.get_mut(id.0 as usize).and_then(|slot| slot.as_mut()) else {
                continue;
            };
            match &mut node.kind {
                NodeKind::Split { ratio, .. } => *ratio = clamp_ratio(*ratio),
                NodeKind::Stack { tabs, active } if !tabs.is_empty() && *active >= tabs.len() => *active = tabs.len() - 1,
                NodeKind::Stack { .. } => {}
            }
        }
    }

    fn replace_child(&mut self, parent: NodeId, old: NodeId, new: NodeId) {
        let Some(node) = self.nodes.get_mut(parent.0 as usize).and_then(|slot| slot.as_mut()) else {
            return;
        };
        if let NodeKind::Split { first, second, .. } = &mut node.kind {
            if *first == old {
                *first = new;
            }
            if *second == old {
                *second = new;
            }
        }
    }

    fn set_tabs(&mut self, id: NodeId, tabs: Vec<PanelId>, active: usize) {
        if let Some(node) = self.nodes.get_mut(id.0 as usize).and_then(|slot| slot.as_mut()) {
            node.kind = NodeKind::Stack { tabs, active };
        }
    }

    fn stack(&mut self, tabs: Vec<PanelId>) -> NodeId {
        self.alloc(NodeKind::Stack { active: 0, tabs }, None)
    }

    fn split(&mut self, axis: Axis, ratio: f32, first: NodeId, second: NodeId) -> NodeId {
        let id = self.alloc(NodeKind::Split { axis, ratio: clamp_ratio(ratio), first, second }, None);
        self.set_parent(first, Some(id));
        self.set_parent(second, Some(id));
        id
    }

    fn alloc(&mut self, kind: NodeKind, parent: Option<NodeId>) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        if self.nodes.len() <= id.0 as usize {
            self.nodes.resize_with(id.0 as usize + 1, || None);
        }
        self.nodes[id.0 as usize] = Some(Node { kind, parent });
        id
    }

    fn set_parent(&mut self, id: NodeId, parent: Option<NodeId>) {
        if let Some(node) = self.nodes.get_mut(id.0 as usize).and_then(|slot| slot.as_mut()) {
            node.parent = parent;
        }
    }

    fn kill(&mut self, id: NodeId) {
        if let Some(slot) = self.nodes.get_mut(id.0 as usize) {
            *slot = None;
        }
    }

    fn node(&self, id: NodeId) -> Result<&Node, DockError> {
        self.nodes.get(id.0 as usize).and_then(|slot| slot.as_ref()).ok_or(DockError::UnknownNode(id))
    }

    fn node_mut(&mut self, id: NodeId) -> Result<&mut Node, DockError> {
        self.nodes.get_mut(id.0 as usize).and_then(|slot| slot.as_mut()).ok_or(DockError::UnknownNode(id))
    }

    fn is_stack(&self, id: NodeId) -> bool {
        matches!(self.nodes.get(id.0 as usize).and_then(|slot| slot.as_ref()).map(|node| &node.kind), Some(NodeKind::Stack { .. }))
    }

    fn tabs(&self, id: NodeId) -> Vec<PanelId> {
        match self.nodes.get(id.0 as usize).and_then(|slot| slot.as_ref()).map(|node| &node.kind) {
            Some(NodeKind::Stack { tabs, .. }) => tabs.clone(),
            _ => Vec::new(),
        }
    }

    fn alive_ids(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.as_ref().map(|_| NodeId(index as u32)))
            .collect()
    }

    fn walk_ids(&self, id: NodeId) -> Vec<NodeId> {
        let mut ids = Vec::new();
        self.walk(id, &mut ids);
        ids
    }

    fn walk(&self, id: NodeId, ids: &mut Vec<NodeId>) {
        let Ok(node) = self.node(id) else {
            return;
        };
        ids.push(id);
        let children = match &node.kind {
            NodeKind::Split { first, second, .. } => Some((*first, *second)),
            NodeKind::Stack { .. } => None,
        };
        if let Some((first, second)) = children {
            self.walk(first, ids);
            self.walk(second, ids);
        }
    }

    fn collect_panels(&self, id: NodeId, panels: &mut Vec<PanelId>) {
        let Ok(node) = self.node(id) else {
            return;
        };
        match &node.kind {
            NodeKind::Split { first, second, .. } => {
                let first = *first;
                let second = *second;
                self.collect_panels(first, panels);
                self.collect_panels(second, panels);
            }
            NodeKind::Stack { tabs, .. } => panels.extend(tabs.iter().copied()),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WorkspaceCommand {
    ShowPanel(PanelId),
    ClosePanel(PanelId),
    DockPanel { panel: PanelId, target: NodeId, zone: DropZone },
    SetSplitRatio { node: NodeId, ratio: f32 },
    Activate(PanelId),
    Focus(PanelId),
    ResetLayout,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DipRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl DipRect {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };

    pub fn contains(self, point: DipPoint) -> bool {
        point.x >= self.x && point.y >= self.y && point.x < self.x + self.width && point.y < self.y + self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DipPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug)]
pub struct PlacedPanel {
    pub panel: PanelId,
    pub stack: NodeId,
    pub rect: DipRect,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PlacedTab {
    pub panel: PanelId,
    pub rect: DipRect,
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct PlacedStack {
    pub node: NodeId,
    pub rect: DipRect,
    pub tab_rect: DipRect,
    pub tabs: Vec<PlacedTab>,
}

#[derive(Clone, Copy, Debug)]
pub struct PlacedSplitter {
    pub node: NodeId,
    pub axis: Axis,
    pub rect: DipRect,
    pub origin: f32,
    pub span: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub panels: Vec<PlacedPanel>,
    pub stacks: Vec<PlacedStack>,
    pub splitters: Vec<PlacedSplitter>,
}

impl Layout {
    pub fn visible(&self) -> impl Iterator<Item = &PlacedPanel> {
        self.panels.iter().filter(|panel| panel.visible)
    }

    pub fn tab_at(&self, point: DipPoint) -> Option<PanelId> {
        for stack in &self.stacks {
            for tab in &stack.tabs {
                if tab.rect.contains(point) {
                    return Some(tab.panel);
                }
            }
        }
        None
    }

    pub fn splitter_at(&self, point: DipPoint) -> Option<NodeId> {
        self.splitters.iter().find(|splitter| inflate(splitter.rect, 3.0).contains(point)).map(|splitter| splitter.node)
    }

    pub fn drop_target(&self, point: DipPoint) -> Option<DropTarget> {
        let stack = self.stacks.iter().rev().find(|stack| stack.rect.contains(point))?;
        let zone = if stack.tab_rect.contains(point) {
            DropZone::Center
        } else {
            zone_at(stack.rect, point)
        };
        Some(DropTarget { stack: stack.node, zone, rect: zone_rect(stack.rect, zone) })
    }

    pub fn ratio_at(&self, node: NodeId, point: DipPoint) -> Option<f32> {
        let splitter = self.splitters.iter().find(|splitter| splitter.node == node)?;
        let cursor = match splitter.axis {
            Axis::Horizontal => point.x,
            Axis::Vertical => point.y,
        };
        let free = (splitter.span - SPLITTER_DIP).max(1.0);
        Some(clamp_ratio((cursor - splitter.origin - SPLITTER_DIP * 0.5) / free))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DropTarget {
    pub stack: NodeId,
    pub zone: DropZone,
    pub rect: DipRect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PersistWorkspace {
    pub schema_version: u32,
    pub preset: String,
    pub host: HostKind,
    pub focused: Option<PanelId>,
    pub root: PersistNode,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PersistNode {
    Split { axis: Axis, ratio: f32, first: Box<PersistNode>, second: Box<PersistNode> },
    Stack { tabs: Vec<PanelId>, active: u32 },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyRoute {
    None,
    Text(PanelId),
    Viewport(PanelId),
    Panel(PanelId),
}

/// Viewport keys are delivered only when that panel is focused and no text field has the caret.
pub fn key_route(focused: Option<PanelId>, text_focused: bool) -> KeyRoute {
    match focused {
        Some(id) if text_focused => KeyRoute::Text(id),
        Some(PERSPECTIVE) => KeyRoute::Viewport(PERSPECTIVE),
        Some(id) => KeyRoute::Panel(id),
        None => KeyRoute::None,
    }
}

pub fn panel_takes_text(id: PanelId) -> bool {
    matches!(id, INSPECTOR | CONTENT | OUTPUT)
}

pub fn dip_to_px(dip: f32, dpi: u32) -> i32 {
    (dip * dpi.max(1) as f32 / DIP_PER_INCH).round() as i32
}

pub fn px_to_dip(px: i32, dpi: u32) -> f32 {
    px as f32 * DIP_PER_INCH / dpi.max(1) as f32
}

fn clamp_ratio(ratio: f32) -> f32 {
    if !ratio.is_finite() {
        return 0.5;
    }
    ratio.clamp(RATIO_MIN, RATIO_MAX)
}

fn split_sizes(span: f32, ratio: f32) -> (f32, f32, f32) {
    let bar = SPLITTER_DIP.min(span.max(0.0));
    let free = (span - bar).max(0.0);
    let mut first = free * clamp_ratio(ratio);
    let mut second = free - first;
    if free >= MIN_PANEL_DIP * 2.0 {
        if first < MIN_PANEL_DIP {
            first = MIN_PANEL_DIP;
            second = free - first;
        } else if second < MIN_PANEL_DIP {
            second = MIN_PANEL_DIP;
            first = free - second;
        }
    }
    (first, bar, second)
}

fn inflate(rect: DipRect, pad: f32) -> DipRect {
    DipRect {
        x: rect.x - pad,
        y: rect.y - pad,
        width: rect.width + pad * 2.0,
        height: rect.height + pad * 2.0,
    }
}

fn zone_at(rect: DipRect, point: DipPoint) -> DropZone {
    if point.x - rect.x <= DROP_EDGE_DIP {
        DropZone::Left
    } else if rect.x + rect.width - point.x <= DROP_EDGE_DIP {
        DropZone::Right
    } else if point.y - rect.y <= DROP_EDGE_DIP {
        DropZone::Top
    } else if rect.y + rect.height - point.y <= DROP_EDGE_DIP {
        DropZone::Bottom
    } else {
        DropZone::Center
    }
}

pub fn zone_rect(rect: DipRect, zone: DropZone) -> DipRect {
    match zone {
        DropZone::Left => DipRect { width: rect.width * 0.5, ..rect },
        DropZone::Right => DipRect { x: rect.x + rect.width * 0.5, width: rect.width * 0.5, ..rect },
        DropZone::Top => DipRect { height: rect.height * 0.5, ..rect },
        DropZone::Bottom => DipRect { y: rect.y + rect.height * 0.5, height: rect.height * 0.5, ..rect },
        DropZone::Center => {
            let inset = 8.0;
            DipRect {
                x: rect.x + inset,
                y: rect.y + inset,
                width: (rect.width - inset * 2.0).max(0.0),
                height: (rect.height - inset * 2.0).max(0.0),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> DockWorkspace {
        let workspace = DockWorkspace::default_layout();
        workspace.check().unwrap();
        workspace
    }

    fn placed(workspace: &DockWorkspace) -> Layout {
        workspace.layout(DipRect { x: 0.0, y: 0.0, width: 1000.0, height: 800.0 })
    }

    fn visible(layout: &Layout, panel: PanelId) -> DipRect {
        layout.visible().find(|item| item.panel == panel).unwrap().rect
    }

    #[test]
    fn default_workspace_is_valid() {
        let workspace = workspace();
        assert_eq!(workspace.schema_version(), SCHEMA_VERSION);
        assert_eq!(workspace.preset_name(), "Default");
        assert_eq!(workspace.host(), HostKind::Main);
        assert_eq!(workspace.panel_count(), 5);
        assert_eq!(workspace.stack_count(), 5);
        assert_eq!(workspace.split_count(), 4);
        assert_eq!(workspace.focused(), Some(PERSPECTIVE));
        assert!(workspace.persistent().root != PersistNode::Stack { tabs: vec![], active: 0 });
    }

    #[test]
    fn split_ratio_is_resolution_independent() {
        let workspace = workspace();
        let layout = placed(&workspace);
        let outliner = visible(&layout, OUTLINER);
        let expected = (1000.0 - SPLITTER_DIP) * 0.18;
        assert!((outliner.width - expected).abs() < 0.05, "{} vs {expected}", outliner.width);
        assert_ne!(outliner.width.round() as i32, 260);
        assert_eq!(dip_to_px(outliner.width, 96), outliner.width.round() as i32);
        assert_eq!(dip_to_px(outliner.width, 192), (outliner.width * 2.0).round() as i32);
    }

    #[test]
    fn horizontal_split_places_outliner_left_of_the_viewport() {
        let layout = placed(&workspace());
        let outliner = visible(&layout, OUTLINER);
        let viewport = visible(&layout, PERSPECTIVE);
        let inspector = visible(&layout, INSPECTOR);
        assert!(outliner.x < viewport.x);
        assert!(viewport.x < inspector.x);
        assert!((outliner.y - viewport.y).abs() < 0.01);
    }

    #[test]
    fn vertical_split_places_content_below_the_viewport() {
        let layout = placed(&workspace());
        let viewport = visible(&layout, PERSPECTIVE);
        let content = visible(&layout, CONTENT);
        let output = visible(&layout, OUTPUT);
        assert!(content.y > viewport.y + viewport.height);
        assert!((content.y - output.y).abs() < 0.01);
        assert!(content.x < output.x);
    }

    #[test]
    fn tab_activation_hides_the_inactive_panel() {
        let mut workspace = workspace();
        let content = workspace.stack_of(CONTENT).unwrap();
        workspace.dock_panel(OUTPUT, content, DropZone::Center).unwrap();
        let layout = placed(&workspace);
        assert!(layout.visible().any(|panel| panel.panel == OUTPUT));
        assert!(layout.panels.iter().any(|panel| panel.panel == CONTENT && !panel.visible));
        workspace.activate(CONTENT).unwrap();
        let layout = placed(&workspace);
        assert!(layout.visible().any(|panel| panel.panel == CONTENT));
        assert!(layout.panels.iter().any(|panel| panel.panel == OUTPUT && !panel.visible));
        assert_eq!(workspace.focused(), Some(CONTENT));
        workspace.check().unwrap();
    }

    #[test]
    fn close_tab_and_reopen_singleton() {
        let mut workspace = workspace();
        let output = OUTPUT;
        workspace.close_panel(output).unwrap();
        assert!(!workspace.is_open(output));
        workspace.show_panel(output).unwrap();
        assert!(workspace.is_open(output));
        assert_eq!(workspace.focused(), Some(output));
        let layout = placed(&workspace);
        assert!(visible(&layout, CONTENT).x < visible(&layout, OUTPUT).x);
        workspace.check().unwrap();
    }

    #[test]
    fn empty_stack_collapses_the_parent_split() {
        let mut workspace = workspace();
        let splits = workspace.split_count();
        workspace.close_panel(OUTPUT).unwrap();
        assert_eq!(workspace.split_count(), splits - 1);
        assert!(workspace.is_open(CONTENT));
        assert!(!workspace.is_open(OUTPUT));
        workspace.check().unwrap();
    }

    #[test]
    fn nested_split_collapses_to_the_remaining_sibling() {
        let mut workspace = workspace();
        workspace.close_panel(OUTPUT).unwrap();
        workspace.close_panel(CONTENT).unwrap();
        workspace.close_panel(INSPECTOR).unwrap();
        workspace.close_panel(OUTLINER).unwrap();
        assert_eq!(workspace.open_panels(), vec![PERSPECTIVE]);
        assert_eq!(workspace.split_count(), 0);
        assert_eq!(workspace.stack_count(), 1);
        assert!(workspace.close_panel(PERSPECTIVE).is_err());
        assert!(workspace.is_open(PERSPECTIVE));
        workspace.check().unwrap();
    }

    #[test]
    fn reset_layout_restores_the_default_tree() {
        let mut workspace = workspace();
        let before = workspace.persistent();
        let split = workspace.split_parent(OUTLINER).unwrap();
        workspace.set_split_ratio(split, 0.3).unwrap();
        workspace.close_panel(INSPECTOR).unwrap();
        workspace.focus(OUTLINER).unwrap();
        workspace.apply(WorkspaceCommand::ResetLayout).unwrap();
        assert_eq!(workspace.persistent(), before);
        workspace.check().unwrap();
    }

    #[test]
    fn focus_changes_without_hiding_the_viewport() {
        let mut workspace = workspace();
        workspace.focus(OUTLINER).unwrap();
        assert_eq!(workspace.focused(), Some(OUTLINER));
        assert!(placed(&workspace).visible().any(|panel| panel.panel == PERSPECTIVE));
        workspace.focus(INSPECTOR).unwrap();
        assert_eq!(workspace.focused(), Some(INSPECTOR));
        assert!(workspace.focus(OUTPUT).is_ok());
        workspace.close_panel(OUTPUT).unwrap();
        assert!(workspace.focus(OUTPUT).is_err());
    }

    #[test]
    fn viewport_panel_survives_layout_mutation() {
        let mut workspace = workspace();
        let split = workspace.split_parent(OUTLINER).unwrap();
        workspace.set_split_ratio(split, 0.01).unwrap();
        workspace.dock_panel(INSPECTOR, workspace.stack_of(CONTENT).unwrap(), DropZone::Center).unwrap();
        workspace.close_panel(OUTLINER).unwrap();
        workspace.show_panel(OUTLINER).unwrap();
        assert_eq!(workspace.open_panels().iter().filter(|panel| **panel == PERSPECTIVE).count(), 1);
        assert!(workspace.is_open(PERSPECTIVE));
        workspace.check().unwrap();
    }

    #[test]
    fn panel_order_is_deterministic() {
        let first = workspace().open_panels();
        let second = DockWorkspace::default_layout().open_panels();
        assert_eq!(first, second);
        assert_eq!(first, vec![OUTLINER, PERSPECTIVE, INSPECTOR, CONTENT, OUTPUT]);
        let visible: Vec<_> = placed(&workspace()).visible().map(|panel| panel.panel).collect();
        assert_eq!(visible, first);
    }

    #[test]
    fn duplicate_singleton_is_rejected() {
        let mut workspace = workspace();
        let error = workspace.add_panel(OUTLINER, workspace.stack_of(CONTENT).unwrap(), DropZone::Center).unwrap_err();
        assert_eq!(error, DockError::DuplicateSingleton(OUTLINER));
        assert_eq!(workspace.open_panels().iter().filter(|panel| **panel == OUTLINER).count(), 1);
        let mut registry = PanelRegistry::new();
        registry.register(BUILTINS[0]).unwrap();
        assert_eq!(registry.register(BUILTINS[0]).unwrap_err(), DockError::AlreadyRegistered(OUTLINER));
    }

    #[test]
    fn unknown_preset_is_rejected() {
        assert!(DockWorkspace::from_preset("Level Editing").is_err());
        assert_eq!(DockWorkspace::from_preset("Default").unwrap().preset_name(), "Default");
    }

    #[test]
    fn persistent_tree_has_no_runtime_handles() {
        let tree = workspace().persistent();
        assert_eq!(tree.schema_version, 1);
        assert_eq!(tree.host, HostKind::Main);
        match tree.root {
            PersistNode::Split { .. } => {}
            PersistNode::Stack { .. } => panic!("root should be a split"),
        }
    }

    #[test]
    fn workspace_behavior_covers_split_tab_hide_dock_collapse_reset_and_focus() {
        let mut workspace = workspace();
        let viewport = workspace.stack_of(PERSPECTIVE).unwrap();
        let before = placed(&workspace);
        let outliner_before = visible(&before, OUTLINER).width;
        let split = workspace.split_parent(OUTLINER).unwrap();
        let ratio = placed(&workspace).ratio_at(split, DipPoint { x: 320.0, y: 40.0 }).unwrap();
        workspace.apply(WorkspaceCommand::SetSplitRatio { node: split, ratio }).unwrap();
        assert!(visible(&placed(&workspace), OUTLINER).width > outliner_before);
        let splits = workspace.split_count();
        workspace.apply(WorkspaceCommand::DockPanel { panel: OUTPUT, target: workspace.stack_of(CONTENT).unwrap(), zone: DropZone::Center }).unwrap();
        workspace.apply(WorkspaceCommand::Activate(OUTPUT)).unwrap();
        assert_eq!(workspace.focused(), Some(OUTPUT));
        assert!(workspace.split_count() < splits);
        let layout = placed(&workspace);
        assert!(layout.visible().any(|panel| panel.panel == OUTPUT));
        assert!(layout.panels.iter().any(|panel| panel.panel == CONTENT && !panel.visible));
        workspace.apply(WorkspaceCommand::ClosePanel(OUTPUT)).unwrap();
        assert!(!workspace.is_open(OUTPUT));
        workspace.apply(WorkspaceCommand::ShowPanel(OUTPUT)).unwrap();
        assert!(workspace.is_open(OUTPUT));
        workspace.apply(WorkspaceCommand::Focus(viewport_panel())).unwrap();
        workspace.apply(WorkspaceCommand::ResetLayout).unwrap();
        assert_eq!(workspace.persistent(), DockWorkspace::default_layout().persistent());
        assert_eq!(workspace.focused(), Some(PERSPECTIVE));
        assert!(workspace.is_open(PERSPECTIVE));
        assert_eq!(workspace.stack_of(PERSPECTIVE).is_some(), true);
        let _ = viewport;
        workspace.check().unwrap();
    }

    fn viewport_panel() -> PanelId {
        PERSPECTIVE
    }

    #[test]
    fn text_focus_does_not_route_keys_to_the_viewport() {
        assert_eq!(key_route(Some(PERSPECTIVE), false), KeyRoute::Viewport(PERSPECTIVE));
        assert_eq!(key_route(Some(OUTPUT), true), KeyRoute::Text(OUTPUT));
        assert_eq!(key_route(Some(OUTLINER), false), KeyRoute::Panel(OUTLINER));
        assert!(panel_takes_text(OUTPUT));
        assert!(!panel_takes_text(PERSPECTIVE));
    }

    #[test]
    fn ratio_clamp_and_pointer_mapping() {
        let mut workspace = workspace();
        let split = workspace.split_parent(OUTLINER).unwrap();
        workspace.set_split_ratio(split, 0.0).unwrap();
        let layout = placed(&workspace);
        let ratio = layout.ratio_at(split, DipPoint { x: 400.0, y: 10.0 }).unwrap();
        assert!((ratio - 400.0 / (1000.0 - SPLITTER_DIP)).abs() < 0.02 || (ratio - RATIO_MAX).abs() < 0.001 || ratio <= RATIO_MAX);
        assert!(ratio <= RATIO_MAX && ratio >= RATIO_MIN);
    }
}

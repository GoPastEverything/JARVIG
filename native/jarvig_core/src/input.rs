//! Action input. Devices are sampled by the host. Game code reads actions, not keys.
//!
//! A mapping context turns one frame of device state into action values. The same
//! context is what Play-In-Editor and the standalone game evaluate.

use std::collections::{BTreeMap, BTreeSet};

/// What an action stores after one evaluation. Not a physical control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ActionValue {
    Bool(bool),
    Float(f64),
    Vec2(f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Bool,
    Float,
    Vec2,
}

/// A control the host can report. Names are the binding vocabulary, not a scan code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PhysicalControl {
    KeyW,
    KeyA,
    KeyS,
    KeyD,
    KeyQ,
    KeyE,
    Space,
    Shift,
    Control,
    Escape,
    MouseLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputSource {
    Control(PhysicalControl),
    MouseDelta,
    GamepadLeftStick,
    GamepadRightStick,
}

/// Held values stay put for the consumer to integrate. Deltas are already this frame.
/// Rates are per second and are multiplied by the frame delta during evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputIntegration {
    Held,
    Delta,
    Rate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputAction {
    pub name: String,
    pub kind: ActionKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputBinding {
    pub source: InputSource,
    pub action: String,
    pub scale: [f64; 2],
    pub integration: InputIntegration,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputMappingContext {
    pub name: String,
    pub actions: Vec<InputAction>,
    pub bindings: Vec<InputBinding>,
}

/// One host sample. Mouse deltas are pixels since the previous sample. Sticks are -1 to 1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InputDeviceState {
    pub held: BTreeSet<PhysicalControl>,
    pub mouse_dx: f64,
    pub mouse_dy: f64,
    pub left_stick: [f64; 2],
    pub right_stick: [f64; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionState {
    values: BTreeMap<String, ActionValue>,
    just_pressed: BTreeSet<String>,
}

impl InputMappingContext {
    /// The builtin keyboard and mouse set. A project selects it by name. It is not implied.
    pub fn jarvig_default() -> Self {
        let action = |name: &str, kind: ActionKind| InputAction { name: name.into(), kind };
        let key = |control: PhysicalControl, action: &str, scale: [f64; 2]| InputBinding {
            source: InputSource::Control(control),
            action: action.into(),
            scale,
            integration: InputIntegration::Held,
        };
        Self {
            name: "JARVIG.Default".into(),
            actions: vec![
                action("Move", ActionKind::Vec2),
                action("Look", ActionKind::Vec2),
                action("MoveUp", ActionKind::Float),
                action("Sprint", ActionKind::Bool),
                action("Pause", ActionKind::Bool),
                action("Engage", ActionKind::Bool),
            ],
            bindings: vec![
                key(PhysicalControl::KeyW, "Move", [0.0, 1.0]),
                key(PhysicalControl::KeyS, "Move", [0.0, -1.0]),
                key(PhysicalControl::KeyA, "Move", [-1.0, 0.0]),
                key(PhysicalControl::KeyD, "Move", [1.0, 0.0]),
                InputBinding {
                    source: InputSource::GamepadLeftStick,
                    action: "Move".into(),
                    scale: [1.0, 1.0],
                    integration: InputIntegration::Held,
                },
                InputBinding {
                    source: InputSource::MouseDelta,
                    action: "Look".into(),
                    scale: [0.005, 0.005],
                    integration: InputIntegration::Delta,
                },
                InputBinding {
                    source: InputSource::GamepadRightStick,
                    action: "Look".into(),
                    scale: [1.5, 1.5],
                    integration: InputIntegration::Rate,
                },
                key(PhysicalControl::KeyE, "MoveUp", [1.0, 0.0]),
                key(PhysicalControl::Space, "MoveUp", [1.0, 0.0]),
                key(PhysicalControl::KeyQ, "MoveUp", [-1.0, 0.0]),
                key(PhysicalControl::Control, "MoveUp", [-1.0, 0.0]),
                key(PhysicalControl::Shift, "Sprint", [1.0, 0.0]),
                key(PhysicalControl::Escape, "Pause", [1.0, 0.0]),
                key(PhysicalControl::MouseLeft, "Engage", [1.0, 0.0]),
            ],
        }
    }

    pub fn evaluate(&self, device: &InputDeviceState, dt: f64, previously_down: &BTreeSet<String>) -> ActionState {
        let dt = if dt.is_finite() && dt > 0.0 { dt.min(0.1) } else { 0.0 };
        let mut numbers: BTreeMap<String, [f64; 2]> = BTreeMap::new();
        let mut bools: BTreeMap<String, bool> = BTreeMap::new();
        for action in &self.actions {
            match action.kind {
                ActionKind::Bool => {
                    bools.insert(action.name.clone(), false);
                }
                ActionKind::Float | ActionKind::Vec2 => {
                    numbers.insert(action.name.clone(), [0.0, 0.0]);
                }
            }
        }
        for binding in &self.bindings {
            let Some(action) = self.actions.iter().find(|action| action.name == binding.action) else { continue };
            let (x, y) = match binding.source {
                InputSource::Control(control) => {
                    if !device.held.contains(&control) {
                        continue;
                    }
                    (binding.scale[0], binding.scale[1])
                }
                InputSource::MouseDelta => (device.mouse_dx * binding.scale[0], device.mouse_dy * binding.scale[1]),
                InputSource::GamepadLeftStick => (device.left_stick[0] * binding.scale[0], device.left_stick[1] * binding.scale[1]),
                InputSource::GamepadRightStick => (device.right_stick[0] * binding.scale[0], device.right_stick[1] * binding.scale[1]),
            };
            let factor = if binding.integration == InputIntegration::Rate { dt } else { 1.0 };
            match action.kind {
                ActionKind::Bool => {
                    if binding.scale[0] != 0.0 || binding.scale[1] != 0.0 {
                        bools.insert(action.name.clone(), true);
                    }
                }
                ActionKind::Float => {
                    let slot = numbers.entry(action.name.clone()).or_insert([0.0, 0.0]);
                    slot[0] += x * factor;
                }
                ActionKind::Vec2 => {
                    let slot = numbers.entry(action.name.clone()).or_insert([0.0, 0.0]);
                    slot[0] += x * factor;
                    slot[1] += y * factor;
                }
            }
        }
        let mut values = BTreeMap::new();
        let mut down = BTreeSet::new();
        for action in &self.actions {
            let value = match action.kind {
                ActionKind::Bool => {
                    let held = bools.get(&action.name).copied().unwrap_or(false);
                    if held {
                        down.insert(action.name.clone());
                    }
                    ActionValue::Bool(held)
                }
                ActionKind::Float => ActionValue::Float(numbers.get(&action.name).map(|pair| pair[0]).unwrap_or(0.0)),
                ActionKind::Vec2 => {
                    let pair = numbers.get(&action.name).copied().unwrap_or([0.0, 0.0]);
                    ActionValue::Vec2(pair[0], pair[1])
                }
            };
            values.insert(action.name.clone(), value);
        }
        let just_pressed = down.difference(previously_down).cloned().collect();
        ActionState { values, just_pressed }
    }
}

impl ActionState {
    pub fn value(&self, name: &str) -> Option<ActionValue> {
        self.values.get(name).copied()
    }

    pub fn pressed(&self, name: &str) -> bool {
        matches!(self.value(name), Some(ActionValue::Bool(true)))
    }

    pub fn just_pressed(&self, name: &str) -> bool {
        self.just_pressed.contains(name)
    }

    pub fn vec2(&self, name: &str) -> (f64, f64) {
        match self.value(name) {
            Some(ActionValue::Vec2(x, y)) => (x, y),
            _ => (0.0, 0.0),
        }
    }

    pub fn float(&self, name: &str) -> f64 {
        match self.value(name) {
            Some(ActionValue::Float(value)) => value,
            _ => 0.0,
        }
    }

    fn down_names(&self) -> BTreeSet<String> {
        self.values
            .iter()
            .filter_map(|(name, value)| matches!(value, ActionValue::Bool(true)).then(|| name.clone()))
            .collect()
    }
}

impl ActionState {
    pub(crate) fn remember_down(&self) -> BTreeSet<String> {
        self.down_names()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_context_reads_actions_and_not_raw_keys() {
        let context = InputMappingContext::jarvig_default();
        let mut device = InputDeviceState::default();
        device.held.insert(PhysicalControl::KeyW);
        device.held.insert(PhysicalControl::KeyD);
        device.held.insert(PhysicalControl::Shift);
        device.mouse_dx = 100.0;
        device.mouse_dy = -20.0;
        let first = context.evaluate(&device, 1.0 / 60.0, &BTreeSet::new());
        assert_eq!(first.vec2("Move"), (1.0, 1.0));
        assert!((first.vec2("Look").0 - 0.5).abs() < 1.0e-9);
        assert!((first.vec2("Look").1 + 0.1).abs() < 1.0e-9);
        assert!(first.pressed("Sprint"));
        assert!(first.just_pressed("Sprint"));
        let again = context.evaluate(&device, 1.0 / 60.0, &first.remember_down());
        assert!(!again.just_pressed("Sprint"));
        device.held.insert(PhysicalControl::Escape);
        let paused = context.evaluate(&device, 1.0 / 60.0, &again.remember_down());
        assert!(paused.just_pressed("Pause"));
        device.held.remove(&PhysicalControl::KeyW);
        device.held.remove(&PhysicalControl::KeyD);
        device.left_stick = [0.0, 1.0];
        device.mouse_dx = 0.0;
        device.mouse_dy = 0.0;
        let stick = context.evaluate(&device, 0.5, &paused.remember_down());
        assert_eq!(stick.vec2("Move"), (0.0, 1.0));
        device.right_stick = [1.0, 0.0];
        let look = context.evaluate(&device, 0.1, &stick.remember_down());
        assert!((look.vec2("Look").0 - 0.15).abs() < 1.0e-9);
    }
}

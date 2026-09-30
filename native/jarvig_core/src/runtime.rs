use crate::{ClockError, SimulationClock};

/// Host profile. The server never reports a render stage. This is not a GPU backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Profile {
    Editor = 1,
    Client = 2,
    Server = 3,
    Test = 4,
}

impl Profile {
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(Self::Editor),
            2 => Some(Self::Client),
            3 => Some(Self::Server),
            4 => Some(Self::Test),
            _ => None,
        }
    }

    pub fn renders(self) -> bool {
        !matches!(self, Self::Server)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeError {
    BadProfile,
    BadClock(ClockError),
    ShutDown,
    NonFiniteDelta,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuntimeTick {
    pub frame: u64,
    pub fixed_steps: u32,
    pub render_executed: bool,
    pub clamped: bool,
}

/// Headless native runtime. No window, no wgpu, no renderer.
pub struct Runtime {
    profile: Profile,
    clock: SimulationClock,
    running: bool,
}

impl Runtime {
    pub fn new(profile: Profile, fixed_hz: f64, max_steps: u32) -> Result<Self, RuntimeError> {
        let clock = SimulationClock::new(fixed_hz, max_steps).map_err(RuntimeError::BadClock)?;
        Ok(Self {
            profile,
            clock,
            running: true,
        })
    }

    pub fn profile(&self) -> Profile {
        self.profile
    }

    pub fn frame(&self) -> u64 {
        self.clock.frame
    }

    pub fn tick(&mut self, delta_seconds: f64) -> Result<RuntimeTick, RuntimeError> {
        if !self.running {
            return Err(RuntimeError::ShutDown);
        }
        let advance = self.clock.advance(delta_seconds).map_err(|error| match error {
            ClockError::NonFiniteDelta => RuntimeError::NonFiniteDelta,
            other => RuntimeError::BadClock(other),
        })?;
        Ok(RuntimeTick {
            frame: self.clock.frame,
            fixed_steps: advance.fixed_steps,
            render_executed: self.profile.renders(),
            clamped: advance.clamped,
        })
    }

    pub fn shutdown(&mut self) -> Result<(), RuntimeError> {
        if !self.running {
            return Err(RuntimeError::ShutDown);
        }
        self.running = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_does_not_render_and_editor_does() {
        let mut server = Runtime::new(Profile::Server, 60.0, 8).unwrap();
        let tick = server.tick(1.0 / 60.0).unwrap();
        assert_eq!(tick.frame, 1);
        assert!(!tick.render_executed);
        assert!(server.shutdown().is_ok());
        assert_eq!(server.tick(1.0 / 60.0), Err(RuntimeError::ShutDown));

        let mut editor = Runtime::new(Profile::Editor, 60.0, 8).unwrap();
        assert!(editor.tick(1.0 / 60.0).unwrap().render_executed);
    }
}

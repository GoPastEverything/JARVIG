#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockAdvance {
    pub fixed_steps: u32,
    pub alpha: f64,
    pub fixed_delta: f64,
    pub frame_delta: f64,
    pub clamped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockError {
    BadRate,
    BadCap,
    NonFiniteDelta,
}

/// Same rule as the TypeScript prototype: drop leftover time instead of spiraling.
pub struct SimulationClock {
    fixed_delta: f64,
    max_steps: u32,
    accumulator: f64,
    pub frame: u64,
}

impl SimulationClock {
    pub fn new(fixed_hz: f64, max_steps: u32) -> Result<Self, ClockError> {
        if !fixed_hz.is_finite() || fixed_hz <= 0.0 {
            return Err(ClockError::BadRate);
        }
        if max_steps < 1 {
            return Err(ClockError::BadCap);
        }
        Ok(Self {
            fixed_delta: 1.0 / fixed_hz,
            max_steps,
            accumulator: 0.0,
            frame: 0,
        })
    }

    pub fn advance(&mut self, frame_delta_seconds: f64) -> Result<ClockAdvance, ClockError> {
        if !frame_delta_seconds.is_finite() {
            return Err(ClockError::NonFiniteDelta);
        }
        let frame_delta = if frame_delta_seconds > 0.0 {
            frame_delta_seconds
        } else {
            0.0
        };
        self.accumulator += frame_delta;
        let mut fixed_steps = 0u32;
        while self.accumulator + 1e-12 >= self.fixed_delta && fixed_steps < self.max_steps {
            self.accumulator -= self.fixed_delta;
            fixed_steps += 1;
        }
        let clamped = self.accumulator + 1e-12 >= self.fixed_delta;
        if clamped || self.accumulator < 0.0 {
            self.accumulator = 0.0;
        }
        self.frame += 1;
        let alpha = if self.fixed_delta == 0.0 {
            0.0
        } else {
            self.accumulator / self.fixed_delta
        };
        Ok(ClockAdvance {
            fixed_steps,
            alpha,
            fixed_delta: self.fixed_delta,
            frame_delta,
            clamped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_exact_step() {
        let mut clock = SimulationClock::new(60.0, 8).unwrap();
        let step = clock.advance(1.0 / 60.0).unwrap();
        assert_eq!(step.fixed_steps, 1);
        assert!(!step.clamped);
        assert!(step.alpha < 1e-9);
    }

    #[test]
    fn variable_deltas_accumulate() {
        let mut clock = SimulationClock::new(60.0, 8).unwrap();
        assert_eq!(clock.advance(0.5 / 60.0).unwrap().fixed_steps, 0);
        assert_eq!(clock.advance(0.5 / 60.0).unwrap().fixed_steps, 1);
        assert_eq!(clock.advance(1.0 / 60.0).unwrap().fixed_steps, 1);
    }

    #[test]
    fn huge_frame_is_capped() {
        let mut clock = SimulationClock::new(60.0, 8).unwrap();
        let burst = clock.advance(10.0).unwrap();
        assert_eq!(burst.fixed_steps, 8);
        assert!(burst.clamped);
        assert_eq!(clock.advance(1.0 / 60.0).unwrap().fixed_steps, 1);
    }

    #[test]
    fn rejects_non_finite_delta() {
        let mut clock = SimulationClock::new(60.0, 8).unwrap();
        assert_eq!(clock.advance(f64::NAN), Err(ClockError::NonFiniteDelta));
    }
}

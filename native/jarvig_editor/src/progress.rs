//! Editor load progress. Not a scene object and not a renderer pass.
//!
//! `fraction == None` is indeterminate. A number is completed work over a known total.
//! Callers must not invent a percentage when the total is unknown.

#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub active: bool,
    /// Covers the workspace until the load that raised it has finished or failed.
    pub blocking: bool,
    pub phase: String,
    pub detail: String,
    pub fraction: Option<f32>,
    pub failed: Option<String>,
    pub spinner: u32,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            active: false,
            blocking: false,
            phase: String::new(),
            detail: String::new(),
            fraction: None,
            failed: None,
            spinner: 0,
        }
    }
}

impl Progress {
    pub fn begin(&mut self, phase: &str, detail: &str, fraction: Option<f32>) {
        self.active = true;
        self.blocking = true;
        self.failed = None;
        self.phase = phase.to_string();
        self.detail = detail.to_string();
        self.fraction = fraction.map(|value| value.clamp(0.0, 1.0));
    }

    /// Updates the visible line. Does not clear a failure.
    pub fn tick(&mut self, phase: &str, detail: &str, fraction: Option<f32>) {
        self.active = true;
        self.phase = phase.to_string();
        self.detail = detail.to_string();
        self.fraction = fraction.map(|value| value.clamp(0.0, 1.0));
        self.spinner = self.spinner.wrapping_add(1);
    }

    pub fn fail(&mut self, phase: &str, error: &str) {
        self.active = true;
        self.blocking = true;
        self.phase = phase.to_string();
        self.detail = error.to_string();
        self.failed = Some(error.to_string());
        self.fraction = None;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn status_line(&self) -> String {
        if let Some(error) = &self.failed {
            return format!("Failed while {}: {error}", self.phase);
        }
        if !self.active {
            return String::new();
        }
        match self.fraction {
            Some(fraction) => format!("{} — {} {:.0}%", self.phase, self.detail, fraction * 100.0),
            None => format!("{} — {}", self.phase, self.detail),
        }
    }

    pub fn changed(&self, phase: &str, detail: &str) -> bool {
        self.phase != phase || self.detail != detail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_work_is_a_fraction_and_unknown_work_is_not() {
        let mut progress = Progress::default();
        progress.begin("Loading materials", "Tiles101 color", Some(0.25));
        assert!((progress.fraction.unwrap() - 0.25).abs() < 1.0e-6);
        assert!(progress.status_line().contains("25%"));
        progress.tick("Initializing renderer", "Choosing an adapter.", None);
        assert!(progress.fraction.is_none());
        assert!(!progress.status_line().contains('%'));
        progress.fail("Loading materials", "Tiles101 was not found");
        assert_eq!(progress.status_line(), "Failed while Loading materials: Tiles101 was not found");
        progress.clear();
        assert!(!progress.active);
    }
}

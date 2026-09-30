//! Generational slots for RHI resources.
//!
//! A handle is alive, then retired, then physically removed only after the
//! submission that last used it has completed. Slot reuse bumps the generation.
//! This type does not know wgpu.

use crate::{RawHandle, RhiError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Vacant,
    Alive,
    Retired,
}

struct Slot<T> {
    generation: u32,
    phase: Phase,
    last_used: u64,
    bytes: u64,
    label: Option<String>,
    value: Option<T>,
}

pub struct Registry<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
}

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Self { slots: vec![Slot::vacant()], free: Vec::new() }
    }
}

impl<T> Slot<T> {
    fn vacant() -> Self {
        Self {
            generation: 0,
            phase: Phase::Vacant,
            last_used: 0,
            bytes: 0,
            label: None,
            value: None,
        }
    }
}

impl<T> Registry<T> {
    pub fn insert(&mut self, value: T, label: Option<String>, bytes: u64) -> RawHandle {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.phase = Phase::Alive;
            slot.last_used = 0;
            slot.bytes = bytes;
            slot.label = label;
            slot.value = Some(value);
            return RawHandle { index, generation: slot.generation };
        }
        let index = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 1,
            phase: Phase::Alive,
            last_used: 0,
            bytes,
            label,
            value: Some(value),
        });
        RawHandle { index, generation: 1 }
    }

    pub fn get(&self, handle: RawHandle) -> Result<&T, RhiError> {
        Ok(self.alive(handle)?.value.as_ref().unwrap())
    }

    pub fn label(&self, handle: RawHandle) -> Option<String> {
        self.alive(handle).ok().and_then(|slot| slot.label.clone())
    }

    /// Logical release. The value stays until [`collect`] says the GPU is done with it.
    pub fn retire(&mut self, handle: RawHandle) -> Result<(), RhiError> {
        let slot = self.slot_mut(handle)?;
        if slot.phase != Phase::Alive {
            return Err(RhiError::InvalidHandle("resource"));
        }
        slot.phase = Phase::Retired;
        Ok(())
    }

    /// Record that a submission used this resource. Retired resources can still be stamped
    /// so a command buffer recorded before release is not freed early.
    pub fn stamp(&mut self, handle: RawHandle, serial: u64) {
        if let Ok(slot) = self.slot_mut(handle) {
            if slot.phase != Phase::Vacant && serial > slot.last_used {
                slot.last_used = serial;
            }
        }
    }

    pub fn alive_values(&self) -> Vec<(RawHandle, &T)> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.phase == Phase::Alive)
            .map(|(index, slot)| {
                (
                    RawHandle { index: index as u32, generation: slot.generation },
                    slot.value.as_ref().unwrap(),
                )
            })
            .collect()
    }

    pub fn alive_handles(&self) -> Vec<RawHandle> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.phase == Phase::Alive)
            .map(|(index, slot)| RawHandle { index: index as u32, generation: slot.generation })
            .collect()
    }

    /// Remove matching values immediately, from any phase.
    ///
    /// Swapchain image views cannot wait for a fence. DXGI refuses
    /// `ResizeBuffers` while one of them is still alive.
    pub fn extract_where(&mut self, mut predicate: impl FnMut(&T) -> bool) -> Vec<T> {
        let mut extracted = Vec::new();
        for index in 1..self.slots.len() {
            let matches = self.slots[index].value.as_ref().is_some_and(&mut predicate);
            if !matches {
                continue;
            }
            let slot = &mut self.slots[index];
            if let Some(value) = slot.value.take() {
                extracted.push(value);
            }
            if slot.phase == Phase::Vacant {
                continue;
            }
            slot.phase = Phase::Vacant;
            slot.label = None;
            slot.bytes = 0;
            slot.last_used = 0;
            if slot.generation == u32::MAX {
                continue;
            }
            slot.generation += 1;
            self.free.push(index as u32);
        }
        extracted
    }

    /// Physically drop retired values whose last submission has completed.
    /// `completed == 0` still frees resources that were never submitted.
    pub fn collect(&mut self, completed: u64) -> Vec<T> {
        let mut dropped = Vec::new();
        for index in 1..self.slots.len() {
            let slot = &mut self.slots[index];
            if slot.phase != Phase::Retired || slot.last_used > completed {
                continue;
            }
            if let Some(value) = slot.value.take() {
                dropped.push(value);
            }
            slot.phase = Phase::Vacant;
            slot.label = None;
            slot.bytes = 0;
            slot.last_used = 0;
            if slot.generation == u32::MAX {
                continue;
            }
            slot.generation += 1;
            self.free.push(index as u32);
        }
        dropped
    }

    pub fn counts(&self) -> (u32, u32, u64) {
        let mut alive = 0;
        let mut retired = 0;
        let mut bytes = 0;
        for slot in &self.slots {
            match slot.phase {
                Phase::Alive => {
                    alive += 1;
                    bytes += slot.bytes;
                }
                Phase::Retired => retired += 1,
                Phase::Vacant => {}
            }
        }
        (alive, retired, bytes)
    }

    fn alive(&self, handle: RawHandle) -> Result<&Slot<T>, RhiError> {
        let slot = self.slot(handle)?;
        if slot.phase != Phase::Alive {
            return Err(RhiError::InvalidHandle("resource"));
        }
        Ok(slot)
    }

    fn slot(&self, handle: RawHandle) -> Result<&Slot<T>, RhiError> {
        let slot = self.slots.get(handle.index as usize).ok_or(RhiError::InvalidHandle("resource"))?;
        if handle.generation == 0 || slot.generation != handle.generation || slot.phase == Phase::Vacant {
            return Err(RhiError::InvalidHandle("resource"));
        }
        Ok(slot)
    }

    fn slot_mut(&mut self, handle: RawHandle) -> Result<&mut Slot<T>, RhiError> {
        let slot = self.slots.get_mut(handle.index as usize).ok_or(RhiError::InvalidHandle("resource"))?;
        if handle.generation == 0 || slot.generation != handle.generation || slot.phase == Phase::Vacant {
            return Err(RhiError::InvalidHandle("resource"));
        }
        Ok(slot)
    }
}

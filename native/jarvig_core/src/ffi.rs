use crate::{ClockError, Profile, Runtime, RuntimeError, SimulationClock};

/// Opaque clock behind the C ABI. The Rust struct layout is not the plugin ABI.
pub struct JarvigClock {
    inner: SimulationClock,
}

#[repr(C)]
pub struct JarvigClockAdvance {
    pub fixed_steps: u32,
    pub alpha: f64,
    pub fixed_delta: f64,
    pub frame_delta: f64,
    pub clamped: i32,
}

#[no_mangle]
pub extern "C" fn jarvig_clock_create(fixed_hz: f64, max_steps: u32) -> *mut JarvigClock {
    match SimulationClock::new(fixed_hz, max_steps) {
        Ok(inner) => Box::into_raw(Box::new(JarvigClock { inner })),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn jarvig_clock_destroy(clock: *mut JarvigClock) {
    if !clock.is_null() {
        drop(Box::from_raw(clock));
    }
}

#[no_mangle]
pub unsafe extern "C" fn jarvig_clock_advance(
    clock: *mut JarvigClock,
    frame_delta_seconds: f64,
    out: *mut JarvigClockAdvance,
) -> i32 {
    if clock.is_null() || out.is_null() {
        return -1;
    }
    match (*clock).inner.advance(frame_delta_seconds) {
        Ok(step) => {
            *out = JarvigClockAdvance {
                fixed_steps: step.fixed_steps,
                alpha: step.alpha,
                fixed_delta: step.fixed_delta,
                frame_delta: step.frame_delta,
                clamped: i32::from(step.clamped),
            };
            0
        }
        Err(ClockError::NonFiniteDelta) => -1,
        Err(_) => -1,
    }
}

pub struct JarvigRuntime {
    inner: Runtime,
}

#[repr(C)]
pub struct JarvigRuntimeTick {
    pub frame: u64,
    pub fixed_steps: u32,
    pub render_executed: i32,
    pub clamped: i32,
}

#[no_mangle]
pub extern "C" fn jarvig_runtime_create(profile: u32, fixed_hz: f64, max_steps: u32) -> *mut JarvigRuntime {
    let Some(profile) = Profile::from_code(profile) else {
        return std::ptr::null_mut();
    };
    match Runtime::new(profile, fixed_hz, max_steps) {
        Ok(inner) => Box::into_raw(Box::new(JarvigRuntime { inner })),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn jarvig_runtime_tick(
    runtime: *mut JarvigRuntime,
    frame_delta_seconds: f64,
    out: *mut JarvigRuntimeTick,
) -> i32 {
    if runtime.is_null() || out.is_null() {
        return -1;
    }
    match (*runtime).inner.tick(frame_delta_seconds) {
        Ok(tick) => {
            *out = JarvigRuntimeTick {
                frame: tick.frame,
                fixed_steps: tick.fixed_steps,
                render_executed: i32::from(tick.render_executed),
                clamped: i32::from(tick.clamped),
            };
            0
        }
        Err(RuntimeError::ShutDown) => -2,
        Err(RuntimeError::NonFiniteDelta) => -1,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn jarvig_runtime_shutdown(runtime: *mut JarvigRuntime) -> i32 {
    if runtime.is_null() {
        return -1;
    }
    match (*runtime).inner.shutdown() {
        Ok(()) => 0,
        Err(RuntimeError::ShutDown) => -2,
        Err(_) => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn jarvig_runtime_destroy(runtime: *mut JarvigRuntime) {
    if !runtime.is_null() {
        drop(Box::from_raw(runtime));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_abi_matches_one_step() {
        unsafe {
            let clock = jarvig_clock_create(60.0, 8);
            assert!(!clock.is_null());
            let mut out = std::mem::zeroed();
            assert_eq!(jarvig_clock_advance(clock, 1.0 / 60.0, &mut out), 0);
            assert_eq!(out.fixed_steps, 1);
            assert_eq!(jarvig_clock_advance(std::ptr::null_mut(), 0.0, &mut out), -1);
            jarvig_clock_destroy(clock);
            jarvig_clock_destroy(std::ptr::null_mut());
        }
    }

    #[test]
    fn server_runtime_skips_render_stage() {
        unsafe {
            let server = jarvig_runtime_create(3, 60.0, 8);
            assert!(!server.is_null());
            let mut out = std::mem::zeroed();
            assert_eq!(jarvig_runtime_tick(server, 1.0 / 60.0, &mut out), 0);
            assert_eq!(out.frame, 1);
            assert_eq!(out.render_executed, 0);
            assert_eq!(jarvig_runtime_shutdown(server), 0);
            assert_eq!(jarvig_runtime_tick(server, 1.0 / 60.0, &mut out), -2);
            jarvig_runtime_destroy(server);

            let editor = jarvig_runtime_create(1, 60.0, 8);
            assert_eq!(jarvig_runtime_tick(editor, 1.0 / 60.0, &mut out), 0);
            assert_eq!(out.render_executed, 1);
            jarvig_runtime_destroy(editor);
            assert!(jarvig_runtime_create(99, 60.0, 8).is_null());
        }
    }
}

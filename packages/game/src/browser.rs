//! Display-synchronized Emscripten browser integration.
use std::ffi::{c_char, c_int};

unsafe extern "C" {
    fn emscripten_request_animation_frame_loop(
        callback: unsafe extern "C" fn(f64, *mut std::ffi::c_void) -> c_int,
        data: *mut std::ffi::c_void,
    );
    fn emscripten_run_script(script: *const c_char);
    fn emscripten_run_script_int(script: *const c_char) -> c_int;
}

/// Register one owned frame callback. Emscripten invokes it once per animation frame.
/// The callback owns the app until cancellation; no stack references outlive `main`.
pub fn run(frame: impl FnMut(f32) -> bool + 'static) {
    struct Loop {
        frame: Box<dyn FnMut(f32) -> bool>,
        previous: Option<f64>,
    }
    unsafe extern "C" fn animate(timestamp: f64, data: *mut std::ffi::c_void) -> c_int {
        // SAFETY: `data` comes from Box::into_raw below. The animation loop is serial and
        // stops invoking this pointer when we return false. It is freed only on that path.
        let state = unsafe { &mut *data.cast::<Loop>() };
        // RAF may stop entirely while hidden, so a pending visibility checkpoint also
        // resets the clock when the first visible callback arrives.
        // SAFETY: static script reads only our bootstrap's suspension flag.
        if unsafe {
            emscripten_run_script_int(c"Module.fatherloadSuspendRequested ? 1 : 0".as_ptr())
        } != 0
        {
            state.previous = None;
        }
        let delta = if is_hidden() {
            state.previous = None;
            0.0
        } else {
            state
                .previous
                .replace(timestamp)
                .map_or(0.0, |last| (timestamp - last).clamp(0.0, 100.0) / 1000.0)
        };
        #[allow(
            clippy::cast_possible_truncation,
            reason = "frame delta is bounded to 0.1 seconds"
        )]
        let keep_running = (state.frame)(delta as f32);
        if !keep_running {
            // SAFETY: registration transferred this allocation exactly once.
            unsafe {
                drop(Box::from_raw(data.cast::<Loop>()));
            }
        }
        c_int::from(keep_running)
    }
    let data = Box::into_raw(Box::new(Loop {
        frame: Box::new(frame),
        previous: None,
    }));
    // SAFETY: the owned state stays valid until animate returns false and releases it.
    unsafe {
        emscripten_request_animation_frame_loop(animate, data.cast());
    }
}

/// Background tabs must not simulate missed real time.
pub fn is_hidden() -> bool {
    // SAFETY: static NUL-terminated script, no borrowed JavaScript values cross the boundary.
    unsafe { emscripten_run_script_int(c"document.hidden ? 1 : 0".as_ptr()) != 0 }
}

/// Request one checkpoint on tab suspension, not one per hidden frame.
pub fn take_suspend_request() -> bool {
    // SAFETY: static script reads and clears a boolean owned by our bootstrap.
    unsafe {
        emscripten_run_script_int(c"(function(){var pending=Module.fatherloadSuspendRequested;Module.fatherloadSuspendRequested=false;return pending?1:0;})()".as_ptr()) != 0
    }
}

/// Queue the virtual save directory for asynchronous `IndexedDB` persistence.
pub fn persist_saves() {
    // SAFETY: static script, valid until the synchronous JS call returns.
    unsafe {
        emscripten_run_script(
            c"Module.fatherloadSaveDirty=true; Module.flushFatherloadSaves();".as_ptr(),
        );
    }
}

/// Fit the actual render buffer, not only CSS; split-screen layouts use this size.
pub fn resize_canvas(raylib: &mut raylib::RaylibHandle) {
    // SAFETY: browser dimensions are returned as bounded integer pixel counts.
    let (width, height) = unsafe {
        (emscripten_run_script_int(c"Math.max(320, Math.min(2560, window.innerWidth))".as_ptr()),
         emscripten_run_script_int(c"Math.max(240, Math.min(1440, document.fullscreenElement ? window.innerHeight : Math.floor(window.innerHeight-(document.getElementById('canvas').getBoundingClientRect().top+window.scrollY))))".as_ptr()))
    };
    if raylib.get_screen_width() != width || raylib.get_screen_height() != height {
        raylib.set_window_size(width, height);
    }
}

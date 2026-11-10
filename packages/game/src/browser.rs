//! Emscripten browser integration. Asyncify yields the existing raylib loop to the browser.
use std::ffi::{c_char, c_int};

unsafe extern "C" {
    fn emscripten_sleep(milliseconds: u32);
    fn emscripten_run_script(script: *const c_char);
    fn emscripten_run_script_int(script: *const c_char) -> c_int;
}

/// Yield without busy-waiting. Browser builds never use raylib's desktop frame limiter.
pub fn yield_frame() {
    // SAFETY: Emscripten supplies this function; Asyncify is enabled in the web build.
    unsafe {
        emscripten_sleep(16);
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

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

/// Which of `choices` the page's `?key=` query names, if any (the space timing page, Wave 11).
pub fn query_choice(key: &str, choices: &[&str]) -> Option<usize> {
    let list = choices
        .iter()
        .map(|choice| format!("'{choice}'"))
        .collect::<Vec<_>>()
        .join(",");
    let script = std::ffi::CString::new(format!(
        "[{list}].indexOf(new URLSearchParams(location.search).get('{key}'))"
    ))
    .ok()?;
    // SAFETY: a NUL-terminated script built from our own constant names; it only reads the URL.
    let index = unsafe { emscripten_run_script_int(script.as_ptr()) };
    usize::try_from(index).ok()
}

/// Background tabs must not simulate missed real time.
pub fn is_hidden() -> bool {
    // SAFETY: static NUL-terminated script, no borrowed JavaScript values cross the boundary.
    unsafe {
        emscripten_run_script_int(
            c"document.hidden || document.getElementById('help-panel')?.open ? 1 : 0".as_ptr(),
        ) != 0
    }
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

/// Largest render buffer, long side by short side, in device pixels.
const MAX_RENDER_SIZE: (f32, f32) = (2560.0, 1440.0);

/// The render buffer for a `width` x `height` CSS-pixel page at `ratio` device pixels per CSS
/// pixel: device pixels, scaled down evenly if that would exceed [`MAX_RENDER_SIZE`]. Returns
/// the buffer size and the device pixels per CSS pixel actually used.
#[must_use]
pub fn render_size(width: f32, height: f32, ratio: f32) -> ((i32, i32), f32) {
    let (width, height) = (width.max(1.0), height.max(1.0));
    let scale = ratio
        .max(0.25)
        .min(MAX_RENDER_SIZE.0 / width.max(height))
        .min(MAX_RENDER_SIZE.1 / width.min(height));
    #[allow(
        clippy::cast_possible_truncation,
        reason = "bounded by MAX_RENDER_SIZE"
    )]
    let size = (
        ((width * scale).floor() as i32).max(1),
        ((height * scale).floor() as i32).max(1),
    );
    (size, scale)
}

/// Fit the actual render buffer to the page in device pixels, not only CSS (sharp on phones
/// and Retina screens); split-screen layouts use this size. The scaling policy is told the
/// device pixel ratio so sizes meant in points stay finger-sized.
pub fn resize_canvas(raylib: &mut raylib::RaylibHandle) {
    // SAFETY: browser dimensions are returned as bounded integer counts.
    let (width, height, ratio) = unsafe {
        (
            emscripten_run_script_int(
                c"Math.max(1, Math.min(8192, Math.floor(document.getElementById('game').clientWidth)))".as_ptr(),
            ),
            emscripten_run_script_int(
                c"Math.max(1, Math.min(8192, Math.floor(document.getElementById('game').clientHeight)))".as_ptr(),
            ),
            emscripten_run_script_int(
                c"Math.max(250, Math.min(8000, Math.round((window.devicePixelRatio || 1) * 1000)))".as_ptr(),
            ),
        )
    };
    #[allow(clippy::cast_precision_loss, reason = "page sizes are small integers")]
    let ((width, height), scale) = render_size(width as f32, height as f32, ratio as f32 / 1000.0);
    crate::pixel_scale::set_display_scale(scale);
    if raylib.get_screen_width() != width || raylib.get_screen_height() != height {
        raylib.set_window_size(width, height);
    }
}

/// The page's safe-area insets (notch, rounded corners, home indicator) in CSS pixels: left,
/// top, right, bottom.
#[must_use]
pub fn safe_insets() -> [f32; 4] {
    // SAFETY: static scripts returning small bounded integers.
    let inset = |script: &std::ffi::CStr| unsafe {
        #[allow(clippy::cast_precision_loss, reason = "an inset is a small integer")]
        let value = emscripten_run_script_int(script.as_ptr()) as f32;
        value.clamp(0.0, 400.0)
    };
    [
        inset(c"Module.fatherloadInset ? Module.fatherloadInset(0) : 0"),
        inset(c"Module.fatherloadInset ? Module.fatherloadInset(1) : 0"),
        inset(c"Module.fatherloadInset ? Module.fatherloadInset(2) : 0"),
        inset(c"Module.fatherloadInset ? Module.fatherloadInset(3) : 0"),
    ]
}

/// Whether the device's main pointer is a finger (a phone or tablet): touch is then the input
/// to show before anything has been pressed.
#[must_use]
pub fn touch_first() -> bool {
    // SAFETY: static script returning 0 or 1.
    unsafe {
        emscripten_run_script_int(
            c"window.matchMedia && matchMedia('(pointer: coarse)').matches ? 1 : 0".as_ptr(),
        ) != 0
    }
}

/// Tells the page whether a menu covers the game (its own buttons only show then on a touch
/// screen) and whether the game wants typed text (the page then offers a text field that
/// raises the on-screen keyboard).
pub fn set_page_state(menu: bool, text_entry: bool) {
    let script = match (menu, text_entry) {
        (false, false) => c"Module.fatherloadPageState && Module.fatherloadPageState(false, false)",
        (true, false) => c"Module.fatherloadPageState && Module.fatherloadPageState(true, false)",
        (false, true) => c"Module.fatherloadPageState && Module.fatherloadPageState(false, true)",
        (true, true) => c"Module.fatherloadPageState && Module.fatherloadPageState(true, true)",
    };
    // SAFETY: static script, valid until the synchronous JS call returns.
    unsafe { emscripten_run_script(script.as_ptr()) };
}

/// A character typed into the page's text field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypedKey {
    Char(char),
    Backspace,
    /// Done / Enter on the on-screen keyboard.
    Done,
}

/// The next key typed into the page's text field, oldest first.
#[must_use]
pub fn take_typed_key() -> Option<TypedKey> {
    // SAFETY: static script returning one queued code point (0 when empty).
    let code = unsafe {
        emscripten_run_script_int(
            c"Module.fatherloadTextQueue && Module.fatherloadTextQueue.length ? Module.fatherloadTextQueue.shift() : 0".as_ptr(),
        )
    };
    match u32::try_from(code).ok()? {
        0 => None,
        8 => Some(TypedKey::Backspace),
        10 | 13 => Some(TypedKey::Done),
        code => char::from_u32(code)
            .filter(|character| !character.is_control())
            .map(TypedKey::Char),
    }
}

/// How many fingers the page says are on the canvas. raylib keeps a cancelled touch (a system
/// gesture took it over) as if it were still down; the page's own count is the truth.
#[must_use]
pub fn active_touches() -> usize {
    // SAFETY: static script returning a small count.
    let count = unsafe { emscripten_run_script_int(c"Module.fatherloadTouches | 0".as_ptr()) };
    usize::try_from(count).unwrap_or(0)
}

/// A short buzz under the finger, where the browser has one.
pub fn vibrate() {
    // SAFETY: static script, valid until the synchronous JS call returns.
    unsafe {
        emscripten_run_script(c"navigator.vibrate && navigator.vibrate(8)".as_ptr());
    }
}

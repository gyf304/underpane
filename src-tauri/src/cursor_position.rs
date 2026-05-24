//! Cursor position publisher.
//!
//! Exposes a `watch::Receiver` carrying the current global cursor position
//! in physical screen pixels (top-left origin), matching what
//! `AppHandle::cursor_position()` would return.
//!
//! On macOS this is event-driven: an `NSEvent` global+local monitor for
//! `.mouseMoved` updates the channel on every system mouse movement, without
//! needing Accessibility permission. On other platforms we fall back to
//! polling `AppHandle::cursor_position()` at 30 Hz — until a native hook is
//! added later.

use std::sync::LazyLock;
use tauri::PhysicalPosition;
use tokio::sync::watch;

pub static CURSOR_POSITION: LazyLock<watch::Receiver<PhysicalPosition<f64>>> = LazyLock::new(|| {
    let (tx, rx) = watch::channel(PhysicalPosition { x: 0.0, y: 0.0 });

    #[cfg(target_os = "macos")]
    init_macos(tx);
    #[cfg(not(target_os = "macos"))]
    init_polling_fallback(tx);

    rx
});

fn publish(tx: &watch::Sender<PhysicalPosition<f64>>, pos: PhysicalPosition<f64>) {
    tx.send_if_modified(|cur| {
        if cur.x != pos.x || cur.y != pos.y {
            *cur = pos;
            true
        } else {
            false
        }
    });
}

#[cfg(target_os = "macos")]
fn init_macos(tx: watch::Sender<PhysicalPosition<f64>>) {
    // NSEvent monitor APIs must be installed from the main thread.
    let app = crate::app::APP_HANDLE.clone();
    let _ = app.run_on_main_thread(move || {
        install_ns_event_monitors(tx);
    });
}

#[cfg(target_os = "macos")]
fn install_ns_event_monitors(tx: watch::Sender<PhysicalPosition<f64>>) {
    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask};
    use std::ptr::NonNull;

    let tx_global = tx.clone();
    let global_block = RcBlock::new(move |_event: NonNull<NSEvent>| {
        if let Some(pos) = read_cursor_position() {
            publish(&tx_global, pos);
        }
    });

    let tx_local = tx;
    let local_block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        if let Some(pos) = read_cursor_position() {
            publish(&tx_local, pos);
        }
        // Pass the event through unmodified.
        event.as_ptr()
    });

    // The returned monitor objects must outlive the process. We never
    // unregister, so leak them.
    unsafe {
        if let Some(m) = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::MouseMoved,
            &global_block,
        ) {
            std::mem::forget(m);
        }
        if let Some(m) = NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::MouseMoved,
            &local_block,
        ) {
            std::mem::forget(m);
        }
    }
}

/// Reads the current cursor position in physical screen pixels (top-left
/// origin) using AppKit. Returns `None` if no screen is available.
#[cfg(target_os = "macos")]
fn read_cursor_position() -> Option<PhysicalPosition<f64>> {
    use objc2::class;
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_core_foundation::{CGPoint, CGRect};

    unsafe {
        let loc: CGPoint = msg_send![class!(NSEvent), mouseLocation];
        let primary: *mut AnyObject = msg_send![class!(NSScreen), mainScreen];
        if primary.is_null() {
            return None;
        }
        let frame: CGRect = msg_send![primary, frame];
        let scale: f64 = msg_send![primary, backingScaleFactor];
        // NSEvent.mouseLocation is in logical points with bottom-left origin
        // anchored to the primary display. Flip Y and scale to physical px to
        // match Tauri's PhysicalPosition convention.
        let y_top_logical = frame.size.height - loc.y;
        Some(PhysicalPosition {
            x: loc.x * scale,
            y: y_top_logical * scale,
        })
    }
}

#[cfg(not(target_os = "macos"))]
fn init_polling_fallback(tx: watch::Sender<PhysicalPosition<f64>>) {
    let app = crate::app::APP_HANDLE.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / 30.0));
        loop {
            tick.tick().await;
            if let Ok(pos) = app.cursor_position() {
                publish(&tx, pos);
            }
        }
    });
}

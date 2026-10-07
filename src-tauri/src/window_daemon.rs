//! Owns a desktop window for one monitor: places it as the wallpaper and drives
//! its page (pointer input, visibility, focus).
//!
//! A daemon is fully self-contained: the constructor sets everything up and
//! `Drop` tears it down. It subscribes to the shared window/monitor state
//! channels and to the OS pointer itself, so callers only construct and drop it.
//!
//! On macOS a desktop-level window is borderless, never key, and gets no AppKit
//! mouse events, so neither the window nor WebKit's tracking area sees the
//! pointer. A single app-global `NSEvent` monitor (installed lazily, shared by
//! all daemons through a registry) watches the pointer and hands moves to each
//! window's webview through `_simulateMouseMove:`. Because the monitor handler
//! runs on the main thread, no cross-thread marshaling is needed.
//!
//! WebKit drops synthetic input unless the page's window is considered active,
//! so we swizzle `isKeyWindow` to report `YES` for our windows (looked up per
//! window through an associated object) and post a become-key notification.
//! Visibility is left to WebKit's own window occlusion detection, and focus is
//! derived from the system's currently focused window. Swizzling rather than
//! swapping the instance's class matters: swapping breaks tao's `sendEvent:`,
//! which reads the receiver's superclass.

#[cfg(target_os = "macos")]
mod imp {
    use std::collections::HashSet;
    use std::ffi::c_void;
    use std::mem::MaybeUninit;
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};

    use block2::RcBlock;
    use objc2::ffi::{
        class_addMethod, class_getInstanceMethod, method_getTypeEncoding, objc_getAssociatedObject,
        objc_setAssociatedObject, OBJC_ASSOCIATION_RETAIN,
    };
    use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
    use objc2::{class, msg_send, sel};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventMask, NSEventModifierFlags,
        NSEventType, NSWindowCollectionBehavior, NSWorkspace,
        NSWorkspaceDidActivateApplicationNotification,
    };
    use objc2_core_foundation::{CGPoint, CGRect};
    use objc2_core_graphics::{CGWindowLevelForKey, CGWindowLevelKey};
    use objc2_foundation::{
        MainThreadMarker, NSNotification, NSOperationQueue, NSPoint, NSProcessInfo,
    };
    use tauri::{Monitor, WebviewWindow};

    static ORIGINAL_IS_KEY_WINDOW: AtomicUsize = AtomicUsize::new(0);
    static ORIGINAL_OCCLUSION_STATE: AtomicUsize = AtomicUsize::new(0);
    static SWIZZLED: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
    static MOUSE_MONITOR: OnceLock<()> = OnceLock::new();
    static FOCUS_OBSERVER: OnceLock<()> = OnceLock::new();
    static LOGGED_EXCEPTIONS: AtomicUsize = AtomicUsize::new(0);

    /// Fraction of the monitor that must be covered to hide the page.
    const COVERAGE_THRESHOLD: f64 = 0.8;

    static ASSOC_WEBVIEW: u8 = 0;
    static ASSOC_HIDDEN: u8 = 1;

    static REGISTRY: LazyLock<Mutex<Vec<Weak<Mutex<Option<Handle>>>>>> =
        LazyLock::new(|| Mutex::new(Vec::new()));

    fn key(slot: &'static u8) -> *const c_void {
        slot as *const u8 as *const c_void
    }

    fn addr<T: ?Sized>(ptr: &T) -> usize {
        ptr as *const T as *const () as usize
    }

    /// Runs `f`, catching Objective-C exceptions (which Rust's unwinder cannot)
    /// as well as Rust panics, so neither escapes into the caller. Logs the
    /// first few occurrences.
    fn guard(f: impl FnOnce()) {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            objc2::exception::catch(std::panic::AssertUnwindSafe(f))
        }));
        if LOGGED_EXCEPTIONS.load(Ordering::Relaxed) >= 16 {
            return;
        }
        match outcome {
            Ok(Ok(())) => {}
            Ok(Err(exception)) => {
                LOGGED_EXCEPTIONS.fetch_add(1, Ordering::Relaxed);
                match exception {
                    Some(exception) => eprintln!("underpane: objc exception: {exception:?}"),
                    None => eprintln!("underpane: objc exception (nil)"),
                }
            }
            Err(_) => {
                LOGGED_EXCEPTIONS.fetch_add(1, Ordering::Relaxed);
                eprintln!("underpane: handler panicked");
            }
        }
    }

    struct Handle {
        window: usize,
        webview: usize,
        window_number: isize,
        /// Last visibility applied to the page.
        visible: bool,
    }

    struct NativeWindow {
        handle: Arc<Mutex<Option<Handle>>>,
    }

    impl Drop for NativeWindow {
        fn drop(&mut self) {
            // Invalidate the webview pointer before the window goes away. The
            // focus task only holds a `Weak` and exits on its own.
            if let Ok(mut guard) = self.handle.lock() {
                *guard = None;
            }
        }
    }

    /// Owns one desktop window. Construct to start, drop to stop.
    #[derive(Clone)]
    pub struct WindowDaemon(#[allow(dead_code)] Arc<NativeWindow>);

    impl WindowDaemon {
        pub fn new(
            window: &WebviewWindow,
            monitor: &Monitor,
            _index: usize,
        ) -> Result<Self, tauri::Error> {
            let daemon = Arc::new(NativeWindow {
                handle: Arc::new(Mutex::new(None)),
            });

            let owned_window = window.clone();
            let monitor = monitor.clone();
            let handle = daemon.handle.clone();
            window.run_on_main_thread(move || {
                if let Err(e) = set_window_as_background(&owned_window, &monitor) {
                    eprintln!("underpane: failed to set window as background: {e}");
                }
                capture(&owned_window, handle);
            })?;

            // Coverage has no OS event; poll it (focus stays event-based).
            let _ = tauri::async_runtime::spawn(coverage_loop(Arc::downgrade(&daemon)));

            Ok(Self(daemon))
        }
    }

    /// Observes app activations so focus tracks the focused window without
    /// polling. Registered on the main thread.
    fn ensure_focus_observer() {
        FOCUS_OBSERVER.get_or_init(|| unsafe {
            let block = RcBlock::new(|_note: NonNull<NSNotification>| {
                guard(dispatch_focus);
            });
            let center = NSWorkspace::sharedWorkspace().notificationCenter();
            let token = center.addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceDidActivateApplicationNotification),
                None,
                None::<&NSOperationQueue>,
                &block,
            );
            // The observer lives for the whole process; keep the token too.
            std::mem::forget(token);
        });
    }

    /// Whether the desktop holds focus: true when Finder (the desktop) is the
    /// frontmost application. Uses `NSWorkspace`, so no Accessibility grant is
    /// required. Main thread only.
    fn desktop_has_focus() -> bool {
        match NSWorkspace::sharedWorkspace().frontmostApplication() {
            Some(app) => app
                .bundleIdentifier()
                .map(|id| id.to_string() == "com.apple.finder")
                .unwrap_or(false),
            None => true,
        }
    }

    /// Reflects the current focus onto every desktop window.
    fn dispatch_focus() {
        let focused = desktop_has_focus();
        let handles: Vec<Arc<Mutex<Option<Handle>>>> = {
            let mut registry = REGISTRY.lock().unwrap();
            registry.retain(|weak| weak.strong_count() > 0);
            registry.iter().filter_map(|weak| weak.upgrade()).collect()
        };
        for handle in handles {
            apply_focused(&handle, focused);
        }
    }

    /// Makes the page focused or blurred, which drives `document.hasFocus()`
    /// and the `focus`/`blur` events natively. Main thread.
    fn apply_focused(handle: &Arc<Mutex<Option<Handle>>>, focused: bool) {
        guard(|| {
            let Ok(slot) = handle.lock() else {
                return;
            };
            let Some(handle) = slot.as_ref() else {
                return;
            };
            let window = handle.window as *const AnyObject;
            let webview = handle.webview as *const AnyObject;
            unsafe {
                let responder = if focused {
                    Some(&*webview)
                } else {
                    None::<&AnyObject>
                };
                let _: () = msg_send![window, makeFirstResponder: responder];
            }
        });
    }

    /// Captures the webview, marks the window as one we manage, and nudges
    /// WebKit to recompute the page's activity state.
    fn capture(window: &WebviewWindow, handle: Arc<Mutex<Option<Handle>>>) {
        let result = window.with_webview(move |webview| unsafe {
            let wk = webview.inner();
            let ns_window = webview.ns_window();
            if wk.is_null() || ns_window.is_null() {
                return;
            }
            let ns_window = &*(ns_window as *const AnyObject);
            let wk = &*(wk as *const AnyObject);

            let window_number: isize = msg_send![ns_window, windowNumber];
            swizzle(ns_window.class());

            objc_setAssociatedObject(
                ns_window as *const AnyObject as *mut AnyObject,
                key(&ASSOC_WEBVIEW),
                wk as *const AnyObject as *mut AnyObject,
                OBJC_ASSOCIATION_RETAIN,
            );

            *handle.lock().unwrap() = Some(Handle {
                window: addr(ns_window),
                webview: addr(wk),
                window_number,
                visible: true,
            });
            register(&handle);
            ensure_monitor();
            ensure_focus_observer();

            // WebKit only recomputes `WindowIsActive` on a become-key
            // notification, so nudge it now that `isKeyWindow` reports YES.
            post_window_notification(ns_window, c"NSWindowDidBecomeKeyNotification");

            // Apply the current focus to the freshly-registered window.
            dispatch_focus();
        });
        if let Err(e) = result {
            eprintln!("underpane: failed to attach native mouse tracking: {e}");
        }
    }

    fn register(handle: &Arc<Mutex<Option<Handle>>>) {
        let mut registry = REGISTRY.lock().unwrap();
        registry.retain(|weak| weak.strong_count() > 0);
        registry.push(Arc::downgrade(handle));
    }

    /// Installs the app-global mouse monitor once. Its handler runs on the main
    /// thread, so it can drive WebKit directly.
    fn ensure_monitor() {
        MOUSE_MONITOR.get_or_init(|| {
            let block = RcBlock::new(|_event: NonNull<NSEvent>| {
                guard(on_mouse_move);
            });
            let mask = NSEventMask::MouseMoved
                | NSEventMask::LeftMouseDragged
                | NSEventMask::RightMouseDragged
                | NSEventMask::OtherMouseDragged;
            if let Some(monitor) =
                NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &block)
            {
                // Lives for the whole process; the app is single-instance.
                std::mem::forget(monitor);
            }
        });
    }

    fn on_mouse_move() {
        let handles: Vec<Arc<Mutex<Option<Handle>>>> = {
            let mut registry = REGISTRY.lock().unwrap();
            registry.retain(|weak| weak.strong_count() > 0);
            registry.iter().filter_map(|weak| weak.upgrade()).collect()
        };
        for handle in handles {
            // Hold the lock across injection so `Drop` cannot race us.
            let Ok(mut guard) = handle.lock() else {
                continue;
            };
            if let Some(handle) = guard.as_mut() {
                inject(handle);
            }
        }
    }

    /// Injects the pointer into the window using its own native frame and the
    /// current mouse location, so no monitor bookkeeping is needed. WebKit's
    /// mouse-move hit testing drives hover enter/leave itself, so a single move
    /// (even outside the viewport, which clears hover) suffices.
    fn inject(handle: &Handle) {
        let webview = handle.webview as *const AnyObject;
        let responds: Bool =
            unsafe { msg_send![webview, respondsToSelector: sel!(_simulateMouseMove:)] };
        if !responds.as_bool() {
            return;
        }
        unsafe {
            let screen: CGPoint = msg_send![class!(NSEvent), mouseLocation];
            let frame: CGRect = msg_send![handle.window as *const AnyObject, frame];
            // Window base coordinates (bottom-left origin) are exactly what the
            // event wants; the WKWebView fills the window.
            let location = NSPoint {
                x: screen.x - frame.origin.x,
                y: screen.y - frame.origin.y,
            };
            let timestamp = NSProcessInfo::processInfo().systemUptime();
            let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
                NSEventType::MouseMoved,
                location,
                NSEventModifierFlags::empty(),
                timestamp,
                handle.window_number,
                None,
                0,
                0,
                0.0,
            );
            if let Some(event) = event {
                let _: () = msg_send![webview, _simulateMouseMove: &*event];
            }
        }
    }

    /// Stretches `window` over `monitor` at `kCGDesktopWindowLevel + 1` (above
    /// the wallpaper, below Finder icons and all application windows) and makes
    /// it a borderless accessory window.
    fn set_window_as_background(window: &WebviewWindow, monitor: &Monitor) -> anyhow::Result<()> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| anyhow::anyhow!("must be called on the main thread"))?;
        let ns_app = NSApplication::sharedApplication(mtm);

        // Use Tauri's monitor API rather than `fullscreen: true` so the window
        // stays in the normal window level hierarchy and doesn't enter macOS
        // fullscreen mode (which would move it to its own Space).
        window.set_size(*monitor.size())?;
        window.set_position(*monitor.position())?;

        let ns_window = window.ns_window()? as *mut AnyObject;
        unsafe {
            let behavior = NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::IgnoresCycle;
            let _: () = msg_send![ns_window, setCollectionBehavior: behavior];

            // kCGDesktopWindowLevelKey = 3 per <CoreGraphics/CGWindowLevel.h>.
            // +1 puts us above the raw wallpaper layer but still below Finder
            // icons (desktop + 20) and every application or system window.
            let level = CGWindowLevelForKey(CGWindowLevelKey::DesktopWindowLevelKey) + 1;
            let _: () = msg_send![ns_window, setLevel: level as isize];
            let _: () = msg_send![ns_window, setStyleMask: 0usize];

            let _: () =
                msg_send![&*ns_app, setActivationPolicy: NSApplicationActivationPolicy::Accessory];
        }

        Ok(())
    }

    unsafe fn post_window_notification(window: *const AnyObject, name: &std::ffi::CStr) {
        let name: *mut AnyObject =
            unsafe { msg_send![class!(NSString), stringWithUTF8String: name.as_ptr()] };
        let center: *mut AnyObject =
            unsafe { msg_send![class!(NSNotificationCenter), defaultCenter] };
        let _: () = unsafe { msg_send![center, postNotificationName: name, object: window] };
    }

    /// Recomputes coverage on a timer and reflects it onto the page. There is no
    /// OS event for "the desktop got covered", so this is a poll.
    async fn coverage_loop(weak: Weak<NativeWindow>) {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let Some(daemon) = weak.upgrade() else { break };
            let handle = daemon.handle.clone();
            let _ = crate::app::APP_HANDLE.run_on_main_thread(move || {
                guard(|| {
                    if let Some(visible) = compute_visible(&handle) {
                        apply_visible(&handle, visible);
                    }
                });
            });
        }
    }

    fn compute_visible(handle: &Arc<Mutex<Option<Handle>>>) -> Option<bool> {
        let guard = handle.lock().ok()?;
        let handle = guard.as_ref()?;
        let coverage = monitor_coverage(handle.window as *const AnyObject)?;
        Some(coverage < COVERAGE_THRESHOLD)
    }

    /// Makes WebKit treat the page as hidden or visible, which drives
    /// `document.visibilityState` and `visibilitychange` natively. Main thread.
    fn apply_visible(handle: &Arc<Mutex<Option<Handle>>>, visible: bool) {
        guard(|| {
            let Ok(mut slot) = handle.lock() else {
                return;
            };
            let Some(handle) = slot.as_mut() else {
                return;
            };
            if handle.visible == visible {
                return;
            }
            handle.visible = visible;
            let window = handle.window as *const AnyObject;
            unsafe {
                let number: *mut AnyObject =
                    msg_send![class!(NSNumber), numberWithBool: Bool::new(!visible)];
                objc_setAssociatedObject(
                    window as *const AnyObject as *mut AnyObject,
                    key(&ASSOC_HIDDEN),
                    number,
                    OBJC_ASSOCIATION_RETAIN,
                );
                post_window_notification(window, c"NSWindowDidChangeOcclusionStateNotification");
            }
        });
    }

    /// Fraction of the window's monitor covered by other applications' normal
    /// windows. Uses the window's own frame as the monitor rect (no monitor
    /// bookkeeping) and CoreGraphics' on-screen window list.
    fn monitor_coverage(ns_window: *const AnyObject) -> Option<f64> {
        type CFTypeRef = *const c_void;
        type CFDictionaryRef = *const c_void;
        type CFIndex = isize;
        type CGWindowID = u32;

        #[allow(non_upper_case_globals)]
        const kCFNumberSInt32Type: isize = 3;
        #[allow(non_upper_case_globals)]
        const kCGWindowListOptionOnScreenOnly: u32 = 1 << 0;
        #[allow(non_upper_case_globals)]
        const kCGWindowListExcludeDesktopElements: u32 = 1 << 4;
        #[allow(non_upper_case_globals)]
        const kCGNullWindowID: CGWindowID = 0;

        extern "C" {
            static kCGWindowLayer: *const c_void;
            static kCGWindowBounds: *const c_void;
            static kCGWindowOwnerPID: *const c_void;
            fn CFArrayGetCount(arr: CFTypeRef) -> CFIndex;
            fn CFArrayGetValueAtIndex(arr: CFTypeRef, idx: CFIndex) -> CFTypeRef;
            fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> CFTypeRef;
            fn CFNumberGetValue(number: CFTypeRef, the_type: isize, value: *mut c_void) -> bool;
            fn CFRelease(cf: CFTypeRef);
            fn CGWindowListCopyWindowInfo(option: u32, relative_to: CGWindowID) -> CFTypeRef;
            fn CGRectMakeWithDictionaryRepresentation(dict: CFDictionaryRef, rect: *mut CGRect)
                -> bool;
        }

        struct Owned(CFTypeRef);
        impl Drop for Owned {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    unsafe { CFRelease(self.0) };
                }
            }
        }

        unsafe {
            let frame: CGRect = msg_send![ns_window, frame];
            let primary: *mut AnyObject = msg_send![class!(NSScreen), mainScreen];
            if primary.is_null() {
                return None;
            }
            let primary_frame: CGRect = msg_send![primary, frame];

            let (mw, mh) = (frame.size.width, frame.size.height);
            if mw <= 0.0 || mh <= 0.0 {
                return None;
            }
            // Convert the window's bottom-left frame to CoreGraphics top-left
            // coordinates (which the window list uses).
            let (mx, my) = (
                frame.origin.x,
                primary_frame.size.height - (frame.origin.y + frame.size.height),
            );

            let list = Owned(CGWindowListCopyWindowInfo(
                kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
                kCGNullWindowID,
            ));
            if list.0.is_null() {
                return Some(0.0);
            }

            let normal = CGWindowLevelForKey(CGWindowLevelKey::NormalWindowLevelKey);
            let torn_off = CGWindowLevelForKey(CGWindowLevelKey::TornOffMenuWindowLevelKey);
            let our_pid = std::process::id() as i32;

            let count = CFArrayGetCount(list.0);
            let mut clipped: Vec<(f64, f64, f64, f64)> = Vec::new();
            for i in 0..count {
                let info = CFArrayGetValueAtIndex(list.0, i) as CFDictionaryRef;

                let layer_ref = CFDictionaryGetValue(info, kCGWindowLayer);
                if layer_ref.is_null() {
                    continue;
                }
                let mut layer: i32 = 0;
                if !CFNumberGetValue(
                    layer_ref,
                    kCFNumberSInt32Type,
                    &mut layer as *mut i32 as *mut c_void,
                ) {
                    continue;
                }
                if layer < normal || layer >= torn_off {
                    continue;
                }

                let pid_ref = CFDictionaryGetValue(info, kCGWindowOwnerPID);
                if !pid_ref.is_null() {
                    let mut pid: i32 = 0;
                    if CFNumberGetValue(
                        pid_ref,
                        kCFNumberSInt32Type,
                        &mut pid as *mut i32 as *mut c_void,
                    ) && pid == our_pid
                    {
                        continue;
                    }
                }

                let bounds = CFDictionaryGetValue(info, kCGWindowBounds) as CFDictionaryRef;
                if bounds.is_null() {
                    continue;
                }
                let mut rect = MaybeUninit::<CGRect>::uninit();
                if !CGRectMakeWithDictionaryRepresentation(bounds, rect.as_mut_ptr()) {
                    continue;
                }
                let rect = rect.assume_init();

                let ix = rect.origin.x.max(mx);
                let iy = rect.origin.y.max(my);
                let iw = (rect.origin.x + rect.size.width).min(mx + mw) - ix;
                let ih = (rect.origin.y + rect.size.height).min(my + mh) - iy;
                if iw > 0.0 && ih > 0.0 {
                    clipped.push((ix, iy, iw, ih));
                }
            }

            if clipped.is_empty() {
                return Some(0.0);
            }

            let mut xs: Vec<f64> = Vec::new();
            for &(x, _y, w, _h) in &clipped {
                xs.push(x);
                xs.push(x + w);
            }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let mut area = 0.0;
            for pair in xs.windows(2) {
                let (x0, x1) = (pair[0], pair[1]);
                let width = x1 - x0;
                if width <= 0.0 {
                    continue;
                }
                let mut intervals: Vec<(f64, f64)> = Vec::new();
                for &(x, y, w, h) in &clipped {
                    if x <= x0 && x + w >= x1 {
                        intervals.push((y, y + h));
                    }
                }
                if intervals.is_empty() {
                    continue;
                }
                intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                let mut covered_height = 0.0;
                let (mut start, mut end) = intervals[0];
                for &(next_start, next_end) in &intervals[1..] {
                    if next_start <= end {
                        end = end.max(next_end);
                    } else {
                        covered_height += end - start;
                        start = next_start;
                        end = next_end;
                    }
                }
                covered_height += end - start;
                area += width * covered_height;
            }

            Some((area / (mw * mh)).clamp(0.0, 1.0))
        }
    }

    /// Installs `isKeyWindow` and `occlusionState` on `class`, once per class.
    /// The per-window state is looked up at call time from associated objects,
    /// so every desktop window shares these methods but answers for itself.
    fn swizzle(class: &'static AnyClass) {
        let swizzled = SWIZZLED.get_or_init(|| Mutex::new(HashSet::new()));
        if !swizzled
            .lock()
            .unwrap()
            .insert(class as *const AnyClass as usize)
        {
            return;
        }
        let class_ptr = class as *const AnyClass as *mut AnyClass;
        unsafe {
            let method = class_getInstanceMethod(class, sel!(isKeyWindow));
            if let Some(method) = method.as_ref() {
                ORIGINAL_IS_KEY_WINDOW.store(method.implementation() as usize, Ordering::Relaxed);
                let imp: Imp = std::mem::transmute(
                    is_key_window as unsafe extern "C-unwind" fn(&AnyObject, Sel) -> Bool,
                );
                class_addMethod(
                    class_ptr,
                    sel!(isKeyWindow),
                    imp,
                    method_getTypeEncoding(method),
                );
            }

            let method = class_getInstanceMethod(class, sel!(occlusionState));
            if let Some(method) = method.as_ref() {
                ORIGINAL_OCCLUSION_STATE
                    .store(method.implementation() as usize, Ordering::Relaxed);
                let imp: Imp = std::mem::transmute(
                    occlusion_state as unsafe extern "C-unwind" fn(&AnyObject, Sel) -> usize,
                );
                class_addMethod(
                    class_ptr,
                    sel!(occlusionState),
                    imp,
                    method_getTypeEncoding(method),
                );
            }
        }
    }

    unsafe extern "C-unwind" fn occlusion_state(this: &AnyObject, cmd: Sel) -> usize {
        let hidden = unsafe { objc_getAssociatedObject(this, key(&ASSOC_HIDDEN)) };
        if !hidden.is_null() {
            let hidden: Bool = unsafe { msg_send![hidden, boolValue] };
            if hidden.as_bool() {
                return 0;
            }
        }
        let original = ORIGINAL_OCCLUSION_STATE.load(Ordering::Relaxed);
        if original == 0 {
            return 0;
        }
        let original: unsafe extern "C-unwind" fn(&AnyObject, Sel) -> usize =
            unsafe { std::mem::transmute(original) };
        unsafe { original(this, cmd) }
    }

    unsafe fn associated_webview<'a>(window: &'a AnyObject) -> Option<&'a AnyObject> {
        let webview = unsafe { objc_getAssociatedObject(window, key(&ASSOC_WEBVIEW)) };
        unsafe { webview.as_ref() }
    }

    unsafe extern "C-unwind" fn is_key_window(this: &AnyObject, cmd: Sel) -> Bool {
        if unsafe { associated_webview(this) }.is_some() {
            return Bool::YES;
        }
        let original = ORIGINAL_IS_KEY_WINDOW.load(Ordering::Relaxed);
        if original == 0 {
            return Bool::NO;
        }
        let original: unsafe extern "C-unwind" fn(&AnyObject, Sel) -> Bool =
            unsafe { std::mem::transmute(original) };
        unsafe { original(this, cmd) }
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};

    use tauri::{Monitor, WebviewWindow};
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller;
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::PCWSTR;

    use crate::monitor_info::MONITORS;

    static REGISTRY: LazyLock<Mutex<Vec<Weak<Entry>>>> = LazyLock::new(|| Mutex::new(Vec::new()));
    static HOOK: OnceLock<()> = OnceLock::new();

    /// Fraction of the monitor that must be covered by other windows to hide
    /// the page.
    const COVERAGE_THRESHOLD: f64 = 0.8;

    /// Per-window state the OS hooks reach into to post input and focus.
    struct Entry {
        index: usize,
        window: WebviewWindow,
        /// Top-level Tauri window; input targets are searched from it.
        top: Mutex<isize>,
        /// Last time a pointer move was forwarded, to throttle CDP calls.
        last_inject: Mutex<std::time::Instant>,
        /// Whether the cursor is currently over this monitor's page.
        pointer_inside: std::sync::atomic::AtomicBool,
    }

    /// Owns one desktop window. Construct to start, drop to stop.
    #[derive(Clone)]
    pub struct WindowDaemon(#[allow(dead_code)] Arc<Entry>);

    impl WindowDaemon {
        pub fn new(
            window: &WebviewWindow,
            monitor: &Monitor,
            index: usize,
        ) -> Result<Self, tauri::Error> {
            let entry = Arc::new(Entry {
                index,
                window: window.clone(),
                top: Mutex::new(0),
                last_inject: Mutex::new(std::time::Instant::now()),
                pointer_inside: std::sync::atomic::AtomicBool::new(false),
            });

            register(&entry);
            ensure_hook();

            let owned_window = window.clone();
            let monitor = monitor.clone();
            let entry_for_setup = entry.clone();
            window.run_on_main_thread(move || {
                match set_window_as_background(&owned_window, &monitor) {
                    Ok(top_hwnd) => *entry_for_setup.top.lock().unwrap() = top_hwnd,
                    Err(e) => eprintln!("underpane: failed to set window as background: {e}"),
                }
                // Apply the current focus to the freshly-registered window.
                dispatch_focus();
            })?;

            // Visibility has no OS event either; poll the desktop coverage.
            let _ = tauri::async_runtime::spawn(coverage_loop(Arc::downgrade(&entry)));
            // A child of Progman is not repositioned by the OS when the monitor
            // layout changes, so follow the monitor channel ourselves.
            let _ = tauri::async_runtime::spawn(monitor_loop(Arc::downgrade(&entry)));

            Ok(Self(entry))
        }
    }

    /// Whether the desktop holds focus: true when there is no foreground window
    /// or the foreground window is the shell desktop.
    fn desktop_has_focus() -> bool {
        match foreground_class() {
            None => true,
            Some(class) => matches!(
                class.as_str(),
                "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            ),
        }
    }

    /// Class of the current foreground window, if any.
    fn foreground_class() -> Option<String> {
        use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};

        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut class_buf = [0u16; 256];
            let n = GetClassNameW(hwnd, &mut class_buf);
            (n > 0).then(|| String::from_utf16_lossy(&class_buf[..n as usize]))
        }
    }

    /// Reflects the current focus onto every desktop window. Runs from the
    /// foreground-change event (or the initial setup), never on a timer.
    fn dispatch_focus() {
        let focused = desktop_has_focus();
        for entry in entries() {
            apply_focus(&entry, focused);
        }
    }

    /// Active desktop windows, pruning dead registry entries.
    fn entries() -> Vec<Arc<Entry>> {
        let Ok(mut registry) = REGISTRY.lock() else {
            return Vec::new();
        };
        registry.retain(|weak| weak.strong_count() > 0);
        registry.iter().filter_map(|weak| weak.upgrade()).collect()
    }

    fn register(entry: &Arc<Entry>) {
        let Ok(mut registry) = REGISTRY.lock() else {
            return;
        };
        registry.retain(|weak| weak.strong_count() > 0);
        registry.push(Arc::downgrade(entry));
    }

    /// Runs `f` with the window's webview controller. `with_webview` marshals
    /// to the main thread, so this is safe to call from any thread.
    fn with_controller(
        window: &WebviewWindow,
        f: impl FnOnce(&ICoreWebView2Controller) + Send + 'static,
    ) {
        let window = window.clone();
        let _ = window.with_webview(move |webview| f(&webview.controller()));
    }

    fn call_cdp(controller: &ICoreWebView2Controller, method: &str, params: &str) {
        unsafe {
            let Ok(core) = controller.CoreWebView2() else {
                return;
            };
            let method = wide(method);
            let params = wide(params);
            let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
                |_code, _result| Ok(()),
            ));
            let _ = core.CallDevToolsProtocolMethod(
                PCWSTR(method.as_ptr()),
                PCWSTR(params.as_ptr()),
                &handler,
            );
        }
    }

    fn eval_js(window: &WebviewWindow, expression: String) {
        let params =
            serde_json::json!({ "expression": expression, "returnByValue": true }).to_string();
        with_controller(window, move |controller| {
            call_cdp(controller, "Runtime.evaluate", &params);
        });
    }

    fn dispatch_mouse(window: &WebviewWindow, x: f64, y: f64) {
        let params = mouse_params(x, y);
        with_controller(window, move |controller| {
            call_cdp(controller, "Input.dispatchMouseEvent", &params);
        });
    }

    fn mouse_params(x: f64, y: f64) -> String {
        format!(
            r#"{{"type":"mouseMoved","x":{x:.2},"y":{y:.2},"button":"none","buttons":0,"clickCount":0}}"#
        )
    }

    /// The scale WebView2 input coordinates are expressed in. The webview can
    /// rasterize at a scale that differs from the monitor's, since WebView2
    /// inherits the parent window's DPI.
    fn rasterization_scale(controller: &ICoreWebView2Controller) -> Option<f64> {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller3;
        use windows::core::Interface;

        let c3 = controller.cast::<ICoreWebView2Controller3>().ok()?;
        let mut scale = 0.0f64;
        unsafe { c3.RasterizationScale(&mut scale) }.ok()?;
        (scale > 0.0).then_some(scale)
    }

    /// Makes the page report focused (or not) without touching real OS focus,
    /// so `document.hasFocus()` and the `focus`/`blur` events follow the
    /// desktop. A child of Progman can never be the active window, and CDP's
    /// focus emulation cannot be turned back off, so the page's `hasFocus` is
    /// overridden directly.
    fn apply_focus(entry: &Entry, focused: bool) {
        let expr = format!(
            "(function(){{const f={focused};if(window.__underpaneFocused===f)return;window.__underpaneFocused=f;Object.defineProperty(Document.prototype,'hasFocus',{{configurable:true,value:function(){{return window.__underpaneFocused;}}}});window.dispatchEvent(new Event(f?'focus':'blur'));}})();"
        );
        eval_js(&entry.window, expr);
    }

    /// Marks the page hidden while its monitor is (almost) fully covered by
    /// other windows. WebView2 exposes no visibility override, so the page's
    /// `document.visibilityState`/`hidden` are shimmed and `visibilitychange`
    /// is dispatched, mirroring the macOS occlusion handling.
    fn apply_visible(entry: &Entry, visible: bool) {
        let expr = format!(
            "(function(){{const v={visible};if(window.__underpaneVisibility===v)return;window.__underpaneVisibility=v;Object.defineProperty(Document.prototype,'visibilityState',{{configurable:true,get:function(){{return window.__underpaneVisibility?'visible':'hidden';}}}});Object.defineProperty(Document.prototype,'hidden',{{configurable:true,get:function(){{return !window.__underpaneVisibility;}}}});document.dispatchEvent(new Event('visibilitychange'));}})();"
        );
        eval_js(&entry.window, expr);
    }

    /// Polls desktop coverage and reflects it onto the page's visibility. There
    /// is no OS event for "the desktop got covered", so this is a poll.
    async fn coverage_loop(entry: Weak<Entry>) {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let Some(entry) = entry.upgrade() else { break };
            let Some(monitor) = MONITORS.borrow().get(entry.index).cloned() else {
                continue;
            };
            let Some(coverage) = monitor_coverage(&monitor) else {
                continue;
            };
            let visible = coverage < COVERAGE_THRESHOLD;
            apply_visible(&entry, visible);
        }
    }

    /// Fraction of `monitor` covered by other applications' top-level windows.
    fn monitor_coverage(monitor: &Monitor) -> Option<f64> {
        use windows::Win32::Foundation::{HWND, LPARAM, RECT};
        use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindowLongPtrW, GetWindowRect,
            GetWindowThreadProcessId, IsIconic, IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
        };

        let pos = monitor.position();
        let size = monitor.size();
        let (mw, mh) = (size.width as f64, size.height as f64);
        if mw <= 0.0 || mh <= 0.0 {
            return None;
        }
        let (mx, my) = (pos.x as f64, pos.y as f64);

        struct Ctx {
            mx: f64,
            my: f64,
            mw: f64,
            mh: f64,
            clipped: Vec<(f64, f64, f64, f64)>,
        }
        unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
            let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };

            if !unsafe { IsWindowVisible(hwnd) }.as_bool() || unsafe { IsIconic(hwnd) }.as_bool() {
                return windows::core::BOOL(1);
            }

            let mut pid = 0u32;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            if pid == std::process::id() {
                return windows::core::BOOL(1);
            }

            // The shell desktop and taskbar are not "covering" windows.
            let mut class_buf = [0u16; 256];
            let n = unsafe { GetClassNameW(hwnd, &mut class_buf) };
            if n > 0 {
                let class = String::from_utf16_lossy(&class_buf[..n as usize]);
                if matches!(
                    class.as_str(),
                    "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
                ) {
                    return windows::core::BOOL(1);
                }
            }

            let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
            if ex & WS_EX_TOOLWINDOW.0 as isize != 0 {
                return windows::core::BOOL(1);
            }

            let mut cloaked = 0u32;
            let _ = unsafe {
                DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_CLOAKED,
                    &mut cloaked as *mut u32 as *mut std::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                )
            };
            if cloaked != 0 {
                return windows::core::BOOL(1);
            }

            let mut rect = RECT::default();
            if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
                return windows::core::BOOL(1);
            }

            let (rx, ry) = (rect.left as f64, rect.top as f64);
            let (rw, rh) = (
                (rect.right - rect.left) as f64,
                (rect.bottom - rect.top) as f64,
            );
            let (mx, my, mw, mh) = (ctx.mx, ctx.my, ctx.mw, ctx.mh);
            let ix = rx.max(mx);
            let iy = ry.max(my);
            let iw = (rx + rw).min(mx + mw) - ix;
            let ih = (ry + rh).min(my + mh) - iy;
            if iw > 0.0 && ih > 0.0 {
                ctx.clipped.push((ix, iy, iw, ih));
            }
            windows::core::BOOL(1)
        }

        let mut ctx = Ctx {
            mx,
            my,
            mw,
            mh,
            clipped: Vec::new(),
        };
        let _ = unsafe { EnumWindows(Some(cb), LPARAM(&mut ctx as *mut _ as isize)) };

        Some((union_area(&ctx.clipped) / (mw * mh)).clamp(0.0, 1.0))
    }

    /// Area of the union of axis-aligned rectangles, by a vertical sweep line.
    fn union_area(rects: &[(f64, f64, f64, f64)]) -> f64 {
        if rects.is_empty() {
            return 0.0;
        }
        let mut xs: Vec<f64> = Vec::new();
        for &(x, _y, w, _h) in rects {
            xs.push(x);
            xs.push(x + w);
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mut area = 0.0;
        for pair in xs.windows(2) {
            let (x0, x1) = (pair[0], pair[1]);
            let width = x1 - x0;
            if width <= 0.0 {
                continue;
            }
            let mut intervals: Vec<(f64, f64)> = Vec::new();
            for &(x, y, w, h) in rects {
                if x <= x0 && x + w >= x1 {
                    intervals.push((y, y + h));
                }
            }
            if intervals.is_empty() {
                continue;
            }
            intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let (mut start, mut end) = intervals[0];
            let mut covered = 0.0;
            for &(ns, ne) in &intervals[1..] {
                if ns <= end {
                    end = end.max(ne);
                } else {
                    covered += end - start;
                    start = ns;
                    end = ne;
                }
            }
            covered += end - start;
            area += width * covered;
        }
        area
    }

    /// Repositions the window when the monitor layout changes, since the OS
    /// does not move a `WS_CHILD` of Progman when its monitor moves.
    async fn monitor_loop(entry: Weak<Entry>) {
        let mut monitors_rx = MONITORS.clone();
        loop {
            if monitors_rx.changed().await.is_err() {
                break;
            }
            let Some(entry) = entry.upgrade() else { break };
            let Some(monitor) = MONITORS.borrow().get(entry.index).cloned() else {
                continue;
            };
            let top = entry.top.lock().map(|guard| *guard).unwrap_or(0);
            if top != 0 {
                reposition(top, &monitor);
            }
        }
    }

    fn reposition(top: isize, monitor: &Monitor) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetParent, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
        };

        let hwnd = HWND(top as *mut std::ffi::c_void);
        let (cx, cy) = match unsafe { GetParent(hwnd) } {
            Ok(parent) if !parent.is_invalid() => client_origin(parent),
            _ => (0, 0),
        };
        let pos = monitor.position();
        let size = monitor.size();
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                None,
                pos.x - cx,
                pos.y - cy,
                size.width as i32,
                size.height as i32,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        };
    }

    /// Installs a `WH_MOUSE_LL` low-level mouse hook once, on a dedicated thread
    /// (the hook is delivered to the installing thread, so it needs a message
    /// loop). The callback posts moves to every registered window.
    fn ensure_hook() {
        HOOK.get_or_init(|| {
            let _ = std::thread::Builder::new()
                .name("underpane-mouse-hook".into())
                .spawn(|| unsafe {
                    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
                    use windows::Win32::UI::WindowsAndMessaging::{
                        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW,
                        TranslateMessage, UnhookWindowsHookEx, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL,
                    };

                    unsafe extern "system" fn hook_proc(
                        code: i32,
                        wparam: WPARAM,
                        lparam: LPARAM,
                    ) -> LRESULT {
                        if code >= 0 {
                            let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
                            dispatch(info.pt.x, info.pt.y);
                        }
                        unsafe { CallNextHookEx(None, code, wparam, lparam) }
                    }

                    match SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), None, 0) {
                        Ok(hook) => {
                            // Foreground-window changes drive focus, event-based.
                            use windows::Win32::Foundation::HWND;
                            use windows::Win32::UI::Accessibility::{
                                SetWinEventHook, HWINEVENTHOOK,
                            };
                            use windows::Win32::UI::WindowsAndMessaging::{
                                EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
                            };
                            unsafe extern "system" fn win_event_proc(
                                _hook: HWINEVENTHOOK,
                                event: u32,
                                _hwnd: HWND,
                                _id_object: i32,
                                _id_child: i32,
                                _thread: u32,
                                _time: u32,
                            ) {
                                if event == EVENT_SYSTEM_FOREGROUND {
                                    dispatch_focus();
                                }
                            }
                            let _ = SetWinEventHook(
                                EVENT_SYSTEM_FOREGROUND,
                                EVENT_SYSTEM_FOREGROUND,
                                None,
                                Some(win_event_proc),
                                0,
                                0,
                                WINEVENT_OUTOFCONTEXT,
                            );

                            let mut msg = MSG::default();
                            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                                let _ = TranslateMessage(&msg);
                                DispatchMessageW(&msg);
                            }
                            let _ = UnhookWindowsHookEx(hook);
                        }
                        Err(e) => {
                            eprintln!("underpane: SetWindowsHookExW(WH_MOUSE_LL) failed: {e}")
                        }
                    }
                });
        });
    }

    /// Posts a move to each registered window whose monitor contains `(x, y)`.
    fn dispatch(x: i32, y: i32) {
        let entries: Vec<Arc<Entry>> = {
            let Ok(mut registry) = REGISTRY.lock() else {
                return;
            };
            registry.retain(|weak| weak.strong_count() > 0);
            registry.iter().filter_map(|weak| weak.upgrade()).collect()
        };
        for entry in entries.iter() {
            let top = entry.top.lock().map(|guard| *guard).unwrap_or(0);
            if top == 0 {
                continue;
            }
            let Some((position, size, scale)) = MONITORS
                .borrow()
                .get(entry.index)
                .map(|monitor| {
                    (
                        *monitor.position(),
                        *monitor.size(),
                        monitor.scale_factor(),
                    )
                })
            else {
                continue;
            };
            let within = x >= position.x
                && y >= position.y
                && x < position.x + size.width as i32
                && y < position.y + size.height as i32;
            if within {
                entry
                    .pointer_inside
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                let origin = window_origin(top).unwrap_or((position.x, position.y));
                inject_pointer(entry, x, y, origin, scale);
            } else if entry
                .pointer_inside
                .swap(false, std::sync::atomic::Ordering::Relaxed)
            {
                // Clear hover when the cursor leaves this monitor.
                inject_leave(entry);
            }
        }
    }

    /// Forwards a pointer move to the page through the WebView2 DevTools
    /// protocol. Window messages cannot be used: the webview is parented
    /// behind the desktop icons, so Chromium's window tracking sees the real
    /// cursor over another window and immediately fires `mouseleave`. CDP
    /// input is injected into the renderer directly, so hover follows the
    /// coordinates we send. `with_webview` marshals to the main thread.
    fn inject_pointer(
        entry: &Entry,
        screen_x: i32,
        screen_y: i32,
        origin: (i32, i32),
        scale: f64,
    ) {
        {
            let Ok(mut last) = entry.last_inject.lock() else {
                return;
            };
            let now = std::time::Instant::now();
            if now.duration_since(*last) < std::time::Duration::from_millis(8) {
                return;
            }
            *last = now;
        }

        let (ox, oy) = origin;
        with_controller(&entry.window, move |controller| {
            let scale = rasterization_scale(controller).unwrap_or(scale);
            let x = (screen_x - ox) as f64 / scale;
            let y = (screen_y - oy) as f64 / scale;
            call_cdp(controller, "Input.dispatchMouseEvent", &mouse_params(x, y));
        });
    }

    /// Injects a move outside the viewport so the page clears hover (fires
    /// `mouseout`/`mouseleave`). CDP has no explicit leave event.
    fn inject_leave(entry: &Entry) {
        dispatch_mouse(&entry.window, -1.0, -1.0);
    }

    /// Screen-space origin of `hwnd`, used to convert a cursor position into the
    /// webview's viewport coordinates.
    fn window_origin(hwnd: isize) -> Option<(i32, i32)> {
        use windows::Win32::Foundation::{HWND, RECT};
        use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

        let mut rect = RECT::default();
        unsafe { GetWindowRect(HWND(hwnd as *mut std::ffi::c_void), &mut rect) }
            .ok()
            .map(|_| (rect.left, rect.top))
    }

    /// Screen-space origin of `hwnd`'s client area. `SetWindowPos` positions a
    /// child relative to its parent's client area, but monitor coordinates are
    /// in screen space, so they must be offset by this. Non-zero whenever the
    /// virtual desktop origin is not `(0, 0)` (a monitor left of/above the
    /// primary).
    fn client_origin(hwnd: windows::Win32::Foundation::HWND) -> (i32, i32) {
        use windows::Win32::Foundation::POINT;
        use windows::Win32::Graphics::Gdi::ClientToScreen;

        let mut pt = POINT { x: 0, y: 0 };
        let _ = unsafe { ClientToScreen(hwnd, &mut pt) };
        (pt.x, pt.y)
    }

    /// Reparents the webview window so it sits between the desktop wallpaper and
    /// the desktop icons. Returns the top-level window.
    fn set_window_as_background(
        window: &WebviewWindow,
        monitor: &Monitor,
    ) -> anyhow::Result<isize> {
        use windows::core::{BOOL, PCWSTR};
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, FindWindowExW, FindWindowW, GetWindowLongPtrW, SendMessageTimeoutW,
            SetParent, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE, HWND_BOTTOM,
            SMTO_NORMAL, SWP_NOACTIVATE, SWP_SHOWWINDOW, WS_CHILD, WS_EX_NOACTIVATE,
            WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_VISIBLE,
        };

        let hwnd = window.hwnd()?;

        let top = unsafe {
            let progman = FindWindowW(PCWSTR(wide("Progman").as_ptr()), PCWSTR::null())?;

            // 0x052C asks Progman to spawn a WorkerW behind the desktop icons.
            let mut result: usize = 0;
            SendMessageTimeoutW(
                progman,
                0x052C,
                WPARAM(0xD),
                LPARAM(0x1),
                SMTO_NORMAL,
                1000,
                Some(&mut result as *mut _ as *mut _),
            );

            // Find SHELLDLL_DefView and the WorkerW immediately following its
            // host top-level window in z-order (the wallpaper slot).
            struct Ctx {
                shell_def_view: HWND,
                worker_w: HWND,
            }
            unsafe extern "system" fn enum_proc(top: HWND, lparam: LPARAM) -> BOOL {
                let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };
                let p = unsafe {
                    FindWindowExW(
                        Some(top),
                        None,
                        PCWSTR(wide("SHELLDLL_DefView").as_ptr()),
                        PCWSTR::null(),
                    )
                };
                if let Ok(p) = p {
                    if !p.is_invalid() {
                        ctx.shell_def_view = p;
                        if let Ok(w) = unsafe {
                            FindWindowExW(
                                None,
                                Some(top),
                                PCWSTR(wide("WorkerW").as_ptr()),
                                PCWSTR::null(),
                            )
                        } {
                            ctx.worker_w = w;
                        }
                    }
                }
                BOOL(1)
            }
            let mut ctx = Ctx {
                shell_def_view: HWND::default(),
                worker_w: HWND::default(),
            };
            let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));

            // Raised desktop: Progman has WS_EX_NOREDIRECTIONBITMAP; the
            // wallpaper WorkerW is a child of Progman.
            let progman_ex = GetWindowLongPtrW(progman, GWL_EXSTYLE);
            let is_raised = (progman_ex & WS_EX_NOREDIRECTIONBITMAP.0 as isize) != 0;
            if is_raised {
                if let Ok(w) = FindWindowExW(
                    Some(progman),
                    None,
                    PCWSTR(wide("WorkerW").as_ptr()),
                    PCWSTR::null(),
                ) {
                    ctx.worker_w = w;
                }
            }

            let style = WS_CHILD.0 | WS_VISIBLE.0;
            SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                ex | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize,
            );

            let parent = if is_raised { progman } else { ctx.worker_w };
            let _ = SetParent(hwnd, Some(parent));

            // On legacy desktops, SetParent does not move WebView2's DComp
            // visuals, so the wallpaper stays invisible. Calling SetParentWindow
            // on a raised desktop would hoist Chrome_WidgetWin_0 above
            // SHELLDLL_DefView and obscure the icons.
            if !is_raised {
                let parent_raw = parent.0 as usize;
                let _ = window.with_webview(move |webview| {
                    let _ = webview
                        .controller()
                        .SetParentWindow(HWND(parent_raw as *mut std::ffi::c_void));
                });
            }

            let pos = monitor.position();
            let size = monitor.size();
            let z = if !ctx.shell_def_view.is_invalid() {
                ctx.shell_def_view
            } else {
                HWND_BOTTOM
            };
            let (cx, cy) = client_origin(parent);
            let _ = SetWindowPos(
                hwnd,
                Some(z),
                pos.x - cx,
                pos.y - cy,
                size.width as i32,
                size.height as i32,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );

            let _ = result;

            hwnd.0 as isize
        };

        Ok(top)
    }

    /// Encodes a string as a NUL-terminated UTF-16 buffer for Win32 wide APIs.
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    use tauri::{Monitor, WebviewWindow};

    #[derive(Clone, Default)]
    pub struct WindowDaemon;

    impl WindowDaemon {
        pub fn new(
            _window: &WebviewWindow,
            _monitor: &Monitor,
            _index: usize,
        ) -> Result<Self, tauri::Error> {
            Ok(Self)
        }
    }
}

pub use imp::WindowDaemon;

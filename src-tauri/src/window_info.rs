use serde::Serialize;
use tauri::{LogicalPosition, LogicalRect, LogicalSize};
use std::sync::{LazyLock, OnceLock};
use tokio::sync::watch;

#[derive(Clone, Debug, Serialize)]
pub struct WindowInfo {
    pub id: u32,
    pub rect: LogicalRect<f64, f64>,
    pub focused: bool,
}

impl PartialEq for WindowInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.focused == other.focused
            && self.rect.position.x == other.rect.position.x
            && self.rect.position.y == other.rect.position.y
            && self.rect.size.width == other.rect.size.width
            && self.rect.size.height == other.rect.size.height
    }
}

static HANDLE: OnceLock<tauri::async_runtime::JoinHandle<()>> = OnceLock::new();

pub static WINDOWS: LazyLock<watch::Receiver<Vec<WindowInfo>>> = LazyLock::new(|| {
    let windows = get_all_windows();
    let windows_clone = windows.clone();
    let (tx, rx) = watch::channel(windows);

    HANDLE.get_or_init(|| tauri::async_runtime::spawn(async move {
        let mut prev_windows = windows_clone;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let windows = get_all_windows();
            if windows != prev_windows {
                prev_windows = windows.clone();
                let _ = tx.send(windows);
            }
        }
    }));

    rx
});

/// Returns a list of all normal-level windows across all monitors.
#[cfg(target_os = "macos")]
pub fn get_all_windows_macos() -> Vec<WindowInfo> {
    use objc2_core_graphics::{CGWindowLevelForKey, CGWindowLevelKey};
    use std::ffi::c_void;
    use std::mem::MaybeUninit;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    type CFIndex = isize;
    type CFTypeRef = *const c_void;
    type CFDictionaryRef = *const c_void;
    type CGWindowID = u32;

    /// RAII wrapper for Core Foundation objects.
    struct CFOwned(CFTypeRef);

    impl CFOwned {
        /// Takes ownership of a CF object. Returns `None` if the pointer is null.
        unsafe fn new(ptr: CFTypeRef) -> Option<Self> {
            if ptr.is_null() { None } else { Some(Self(ptr)) }
        }

        fn as_ptr(&self) -> CFTypeRef {
            self.0
        }
    }

    impl Drop for CFOwned {
        fn drop(&mut self) {
            unsafe { CFRelease(self.0); }
        }
    }

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
        static kCGWindowNumber: *const c_void;
        fn CFArrayGetCount(arr: CFTypeRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(arr: CFTypeRef, idx: CFIndex) -> CFTypeRef;
        fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> CFTypeRef;
        fn CFNumberGetValue(number: CFTypeRef, the_type: isize, value: *mut c_void) -> bool;
        fn CFRelease(cf: CFTypeRef);
        fn CGWindowListCopyWindowInfo(option: u32, relative_to: CGWindowID) -> CFTypeRef;
        fn CGRectMakeWithDictionaryRepresentation(dict: CFDictionaryRef, rect: *mut CGRect)
            -> bool;
    }

    type CFStringRef = *const c_void;

    extern "C" {
        fn CFStringCreateWithCString(
            alloc: CFTypeRef,
            c_str: *const u8,
            encoding: u32,
        ) -> CFStringRef;
        fn AXUIElementCreateSystemWide() -> CFTypeRef;
        fn AXUIElementCopyAttributeValue(
            element: CFTypeRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
        fn _AXUIElementGetWindow(element: CFTypeRef, wid: *mut u32) -> i32;
    }

    #[allow(non_upper_case_globals)]
    const kCFStringEncodingUTF8: u32 = 0x08000100;

    // Get the focused window ID using AX API
    let focused_window_id: Option<u32> = (|| unsafe {
        let ax_focused_app = CFOwned::new(CFStringCreateWithCString(
            std::ptr::null(),
            b"AXFocusedApplication\0".as_ptr(),
            kCFStringEncodingUTF8,
        ));
        let ax_focused_window = CFOwned::new(CFStringCreateWithCString(
            std::ptr::null(),
            b"AXFocusedWindow\0".as_ptr(),
            kCFStringEncodingUTF8,
        ));

        let (Some(ax_focused_app), Some(ax_focused_window)) = (ax_focused_app, ax_focused_window)
        else {
            return None;
        };

        let system_wide = CFOwned::new(AXUIElementCreateSystemWide())?;

        let mut focused_app: CFTypeRef = std::ptr::null();
        if AXUIElementCopyAttributeValue(
            system_wide.as_ptr(),
            ax_focused_app.as_ptr(),
            &mut focused_app as *mut _ as *mut CFTypeRef,
        ) != 0 {
            return None;
        }
        let focused_app = CFOwned::new(focused_app)?;

        let mut focused_window: CFTypeRef = std::ptr::null();
        if AXUIElementCopyAttributeValue(
            focused_app.as_ptr(),
            ax_focused_window.as_ptr(),
            &mut focused_window as *mut _ as *mut CFTypeRef,
        ) != 0 {
            return None;
        }
        let focused_window = CFOwned::new(focused_window)?;

        let mut window_id: u32 = 0;
        _AXUIElementGetWindow(focused_window.as_ptr(), &mut window_id);
        (window_id != 0).then_some(window_id)
    })();

    unsafe {
        let windows_ptr = CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        );

        let Some(windows) = CFOwned::new(windows_ptr) else {
            return Vec::new();
        };

        let count = CFArrayGetCount(windows.as_ptr());
        let mut window_infos = Vec::new();

        for i in 0..count {
            let info = CFArrayGetValueAtIndex(windows.as_ptr(), i) as CFDictionaryRef;

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
            if layer < CGWindowLevelForKey(CGWindowLevelKey::NormalWindowLevelKey)
                || layer >= CGWindowLevelForKey(CGWindowLevelKey::TornOffMenuWindowLevelKey)
            {
                continue;
            }

            let number_ref = CFDictionaryGetValue(info, kCGWindowNumber);
            if number_ref.is_null() {
                continue;
            }
            let mut window_id: u32 = 0;
            if !CFNumberGetValue(
                number_ref,
                kCFNumberSInt32Type,
                &mut window_id as *mut u32 as *mut c_void,
            ) {
                continue;
            }

            let bounds_ref = CFDictionaryGetValue(info, kCGWindowBounds) as CFDictionaryRef;
            if bounds_ref.is_null() {
                continue;
            }
            let mut rect = MaybeUninit::<CGRect>::uninit();
            if !CGRectMakeWithDictionaryRepresentation(bounds_ref, rect.as_mut_ptr()) {
                continue;
            }
            let rect = rect.assume_init();

            let focused = focused_window_id == Some(window_id);

            window_infos.push(WindowInfo {
                id: window_id,
                rect: LogicalRect {
                    position: LogicalPosition {
                        x: rect.origin.x,
                        y: rect.origin.y,
                    },
                    size: LogicalSize {
                        width: rect.size.width,
                        height: rect.size.height,
                    },
                },
                focused,
            });
        }

        window_infos
    }
}

/// Returns a list of all top-level windows on Windows.
#[cfg(windows)]
pub fn get_all_windows_windows() -> Vec<WindowInfo> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{
        DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
    };
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, IsIconic, IsWindowVisible,
        GWL_EXSTYLE, WS_EX_TOOLWINDOW,
    };

    struct Ctx {
        focused: HWND,
        windows: Vec<WindowInfo>,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };

        unsafe {
            if !IsWindowVisible(hwnd).as_bool() {
                return BOOL(1);
            }
            if IsIconic(hwnd).as_bool() {
                return BOOL(1);
            }

            // Filter cloaked windows (hidden UWP / virtual-desktop windows).
            let mut cloaked: u32 = 0;
            if DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut _ as *mut _,
                std::mem::size_of::<u32>() as u32,
            )
            .is_ok()
                && cloaked != 0
            {
                return BOOL(1);
            }

            // Filter tool windows (also excludes our own desktop wallpaper
            // windows, which we tag with WS_EX_TOOLWINDOW).
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            if (ex & WS_EX_TOOLWINDOW.0 as isize) != 0 {
                return BOOL(1);
            }

            // Filter the shell windows by class name.
            let mut class_buf = [0u16; 256];
            let n = GetClassNameW(hwnd, &mut class_buf);
            if n > 0 {
                let class = String::from_utf16_lossy(&class_buf[..n as usize]);
                if matches!(
                    class.as_str(),
                    "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
                ) {
                    return BOOL(1);
                }
            }

            // Prefer DWM extended frame bounds over GetWindowRect to avoid the
            // invisible drop-shadow margin.
            let mut rect = RECT::default();
            let bounds_ok = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut rect as *mut _ as *mut _,
                std::mem::size_of::<RECT>() as u32,
            )
            .is_ok();
            if !bounds_ok {
                return BOOL(1);
            }

            // Convert physical pixels → logical (CSS-equivalent) coordinates by
            // dividing by the per-window DPI scale.
            let dpi = GetDpiForWindow(hwnd);
            let scale = if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 };

            let id = hwnd.0 as usize as u32;
            let focused = hwnd == ctx.focused;

            ctx.windows.push(WindowInfo {
                id,
                rect: LogicalRect {
                    position: LogicalPosition {
                        x: rect.left as f64 / scale,
                        y: rect.top as f64 / scale,
                    },
                    size: LogicalSize {
                        width: (rect.right - rect.left) as f64 / scale,
                        height: (rect.bottom - rect.top) as f64 / scale,
                    },
                },
                focused,
            });
        }

        BOOL(1)
    }

    let mut ctx = Ctx {
        focused: unsafe { GetForegroundWindow() },
        windows: Vec::new(),
    };

    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
        let _ = EnumWindows(
            Some(enum_proc),
            LPARAM(&mut ctx as *mut _ as isize),
        );
    }

    ctx.windows
}

pub fn get_all_windows() -> Vec<WindowInfo> {
    #[cfg(target_os = "macos")]
    return get_all_windows_macos();

    #[cfg(windows)]
    return get_all_windows_windows();

    #[allow(unreachable_code)]
    Vec::new()
}

pub fn filter_windows(windows: &[WindowInfo], bound: &LogicalRect<f64, f64>) -> Vec<WindowInfo> {
    let bx = bound.position.x;
    let by = bound.position.y;
    let bw = bound.size.width;
    let bh = bound.size.height;

    windows
        .iter()
        .filter(|w| {
            let wx = w.rect.position.x;
            let wy = w.rect.position.y;
            let ww = w.rect.size.width;
            let wh = w.rect.size.height;

            // Check if window rectangle intersects with bound
            // Two rectangles intersect if: left1 < right2 && right1 > left2 && top1 < bottom2 && bottom1 > top2
            wx < bx + bw && wx + ww > bx && wy < by + bh && wy + wh > by
        })
        .cloned()
        .collect()
}

pub fn coverage(windows: &[WindowInfo], bound: &LogicalRect<f64, f64>) -> f64 {
    let bx = bound.position.x as f64;
    let by = bound.position.y as f64;
    let bw = bound.size.width as f64;
    let bh = bound.size.height as f64;

    let monitor_area = bw * bh;
    if monitor_area == 0.0 {
        return 0.0;
    }

    // Filter and clip windows to bound
    let window_rects: Vec<(f64, f64, f64, f64)> = windows
        .iter()
        .filter_map(|w| {
            let wx = w.rect.position.x as f64;
            let wy = w.rect.position.y as f64;
            let ww = w.rect.size.width as f64;
            let wh = w.rect.size.height as f64;

            // Calculate intersection with bound
            let ix = wx.max(bx);
            let iy = wy.max(by);
            let iw = (wx + ww).min(bx + bw) - ix;
            let ih = (wy + wh).min(by + bh) - iy;

            if iw > 0.0 && ih > 0.0 {
                Some((ix, iy, iw, ih))
            } else {
                None
            }
        })
        .collect();

    if window_rects.is_empty() {
        return 0.0;
    }

    // Collect all unique x-coordinates (start and end of each rectangle)
    let mut xs: Vec<f64> = Vec::new();
    for &(x, _y, w, _h) in &window_rects {
        xs.push(x);
        xs.push(x + w);
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // Sweep line: for each vertical strip between consecutive x-coordinates
    let mut total_coverage = 0.0;
    for i in 0..xs.len() - 1 {
        let x_start = xs[i];
        let x_end = xs[i + 1];
        let width = x_end - x_start;

        // Skip zero-width strips
        if width <= 0.0 {
            continue;
        }

        // Find y-intervals that overlap with this strip
        let mut intervals: Vec<(f64, f64)> = Vec::new();
        for &(x, y, w, h) in &window_rects {
            if x <= x_start && x + w >= x_end {
                intervals.push((y, y + h));
            }
        }

        if intervals.is_empty() {
            continue;
        }

        // Sort intervals by start y
        intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        // Merge overlapping intervals and compute total y-coverage
        let mut merged_y = 0.0;
        let mut current_start = intervals[0].0;
        let mut current_end = intervals[0].1;

        for &(start, end) in &intervals[1..] {
            if start <= current_end {
                current_end = current_end.max(end);
            } else {
                merged_y += current_end - current_start;
                current_start = start;
                current_end = end;
            }
        }
        merged_y += current_end - current_start;

        // Add strip area
        total_coverage += width * merged_y;
    }

    (total_coverage / monitor_area).clamp(0.0, 1.0)
}

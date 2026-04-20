use serde::Serialize;
use tauri::{LogicalPosition, LogicalRect, LogicalSize};

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
    let focused_window_id = unsafe {
        // Create CFStringRef for AX attributes
        #[allow(non_snake_case)]
        let kAXFocusedApplicationAttribute = CFStringCreateWithCString(
            std::ptr::null(),
            b"AXFocusedApplication\0".as_ptr(),
            kCFStringEncodingUTF8,
        );
        #[allow(non_snake_case)]
        let kAXFocusedWindowAttribute = CFStringCreateWithCString(
            std::ptr::null(),
            b"AXFocusedWindow\0".as_ptr(),
            kCFStringEncodingUTF8,
        );

        if kAXFocusedApplicationAttribute.is_null() || kAXFocusedWindowAttribute.is_null() {
            return Vec::new();
        }

        let system_wide = AXUIElementCreateSystemWide();
        if system_wide.is_null() {
            CFRelease(kAXFocusedApplicationAttribute);
            CFRelease(kAXFocusedWindowAttribute);
            None
        } else {
            let mut focused_app: CFTypeRef = std::ptr::null();
            let result = AXUIElementCopyAttributeValue(
                system_wide,
                kAXFocusedApplicationAttribute,
                &mut focused_app as *mut _ as *mut CFTypeRef,
            );

            if result != 0 || focused_app.is_null() {
                CFRelease(kAXFocusedApplicationAttribute);
                CFRelease(kAXFocusedWindowAttribute);
                CFRelease(system_wide);
                None
            } else {
                let mut focused_window: CFTypeRef = std::ptr::null();
                let result = AXUIElementCopyAttributeValue(
                    focused_app,
                    kAXFocusedWindowAttribute,
                    &mut focused_window as *mut _ as *mut CFTypeRef,
                );

                if result != 0 || focused_window.is_null() {
                    CFRelease(kAXFocusedApplicationAttribute);
                    CFRelease(kAXFocusedWindowAttribute);
                    CFRelease(focused_app);
                    CFRelease(system_wide);
                    None
                } else {
                    // println!("yo");
                    let mut window_id: u32 = 0;
                    _AXUIElementGetWindow(focused_window, &mut window_id as *mut _);

                    CFRelease(kAXFocusedApplicationAttribute);
                    CFRelease(kAXFocusedWindowAttribute);
                    CFRelease(focused_window);
                    CFRelease(focused_app);
                    CFRelease(system_wide);

                    if window_id != 0 {
                        Some(window_id)
                    } else {
                        None
                    }
                }
            }
        }
    };

    unsafe {
        let windows = CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        );
        if windows.is_null() {
            return Vec::new();
        }

        let count = CFArrayGetCount(windows);
        let mut window_infos = Vec::new();

        for i in 0..count {
            let info = CFArrayGetValueAtIndex(windows, i) as CFDictionaryRef;

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
                || layer >= CGWindowLevelForKey(CGWindowLevelKey::MaximumWindowLevelKey)
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

            // Compare window ID with focused window ID
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

        CFRelease(windows);

        window_infos
    }
}

pub fn get_all_windows() -> Vec<WindowInfo> {
    #[cfg(target_os = "macos")]
    return get_all_windows_macos();

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

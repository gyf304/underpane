use std::sync::LazyLock;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::watch;

use crate::app::APP_HANDLE;

/// The set of active displays, used to drive the config UI and to detect layout
/// changes. On macOS this is a native enumeration; elsewhere it wraps Tauri's.
#[cfg(target_os = "macos")]
mod native {
    use objc2_core_graphics::{
        CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGError, CGGetActiveDisplayList,
    };
    use tauri::{PhysicalPosition, PhysicalSize};

    /// One active display, in the global display coordinate space (points,
    /// top-left origin). Reporting points rather than backing pixels keeps the
    /// arrangement consistent across displays with different backing scales;
    /// scaling each display's position into pixels independently makes
    /// mixed-DPI arrangements overlap.
    #[derive(Clone, PartialEq)]
    pub struct Display {
        position: PhysicalPosition<i32>,
        size: PhysicalSize<u32>,
        scale_factor: f64,
    }

    impl Display {
        pub fn position(&self) -> &PhysicalPosition<i32> {
            &self.position
        }

        pub fn size(&self) -> &PhysicalSize<u32> {
            &self.size
        }

        pub fn scale_factor(&self) -> f64 {
            self.scale_factor
        }
    }

    pub fn displays() -> Vec<Display> {
        let mut count = 0u32;
        if unsafe { CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut count) } != CGError::Success
        {
            return Vec::new();
        }
        let mut ids = vec![0u32; count as usize];
        if unsafe { CGGetActiveDisplayList(count, ids.as_mut_ptr(), &mut count) } != CGError::Success
        {
            return Vec::new();
        }
        ids.truncate(count as usize);

        ids.into_iter()
            .map(|id| {
                let bounds = CGDisplayBounds(id);
                let mode = CGDisplayCopyDisplayMode(id);
                let scale_factor = mode
                    .as_deref()
                    .map(|mode| {
                        let width = CGDisplayMode::width(Some(mode));
                        let pixels = CGDisplayMode::pixel_width(Some(mode));
                        if width == 0 {
                            1.0
                        } else {
                            pixels as f64 / width as f64
                        }
                    })
                    .unwrap_or(1.0);
                Display {
                    position: PhysicalPosition::new(
                        bounds.origin.x as i32,
                        bounds.origin.y as i32,
                    ),
                    size: PhysicalSize::new(bounds.size.width as u32, bounds.size.height as u32),
                    scale_factor,
                }
            })
            .collect()
    }
}

#[cfg(not(target_os = "macos"))]
mod native {
    use tauri::{AppHandle, Monitor};

    pub type Display = Monitor;

    pub fn displays(app: &AppHandle) -> Vec<Display> {
        app.available_monitors().unwrap_or_default()
    }
}

pub use native::Display;

fn displays(app: &tauri::AppHandle) -> Vec<Display> {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        native::displays()
    }
    #[cfg(not(target_os = "macos"))]
    {
        native::displays(app)
    }
}

pub static MONITORS: LazyLock<watch::Receiver<Vec<Display>>> = LazyLock::new(|| {
    let monitors = displays(&APP_HANDLE);
    let (tx, rx) = watch::channel(monitors.clone());

    let app = APP_HANDLE.clone();
    tauri::async_runtime::spawn(async move {
        let mut prev = signature(&monitors);
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let monitors = displays(&app);
            let next = signature(&monitors);
            if next != prev {
                prev = next;
                tx.send(monitors).ok();
            }
        }
    });

    rx
});

/// A cheap key that changes whenever the arrangement changes.
fn signature(displays: &[Display]) -> Vec<(i32, i32, u32, u32, i64)> {
    displays
        .iter()
        .map(|display| {
            let position = display.position();
            let size = display.size();
            (
                position.x,
                position.y,
                size.width,
                size.height,
                (display.scale_factor() * 1000.0) as i64,
            )
        })
        .collect()
}

#[derive(Serialize)]
pub struct MonitorPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Serialize)]
pub struct MonitorSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize)]
pub struct MonitorInfo {
    pub id: String,
    pub position: MonitorPosition,
    pub size: MonitorSize,
}

pub fn current_monitors() -> Vec<MonitorInfo> {
    MONITORS
        .borrow()
        .iter()
        .enumerate()
        .map(|(i, m)| MonitorInfo {
            id: (i + 1).to_string(),
            position: MonitorPosition {
                x: m.position().x,
                y: m.position().y,
            },
            size: MonitorSize {
                width: m.size().width,
                height: m.size().height,
            },
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesktopSample {
    pub monitor_size: [f32; 2],
    pub window_fraction: [f32; 4],
}

#[cfg(target_os = "windows")]
mod platform {
    use std::{
        mem::size_of,
        path::PathBuf,
        sync::atomic::{AtomicBool, Ordering},
    };

    use super::DesktopSample;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
            Graphics::Dwm::{
                DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_NCRENDERING_POLICY,
                DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DwmSetWindowAttribute,
            },
            Graphics::Gdi::{
                CreateRoundRectRgn, GetMonitorInfoW, GetWindowRgnBox, MONITOR_DEFAULTTONEAREST,
                MONITORINFO, MonitorFromWindow, RGN_ERROR, ScreenToClient, SetWindowRgn,
            },
            UI::{
                Shell::{DefSubclassProc, SetWindowSubclass},
                WindowsAndMessaging::{
                    EnumWindows, FindWindowExW, FindWindowW, GWL_EXSTYLE, GWL_STYLE, GetCursorPos,
                    GetParent, GetWindowLongPtrW, GetWindowRect, HWND_TOP, IsWindow, SMTO_NORMAL,
                    SPI_GETDESKWALLPAPER, STYLESTRUCT, SWP_FRAMECHANGED, SWP_NOACTIVATE,
                    SWP_NOCOPYBITS, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
                    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SendMessageTimeoutW, SetForegroundWindow,
                    SetParent, SetWindowLongPtrW, SetWindowPos, SystemParametersInfoW,
                    WM_ERASEBKGND, WM_NCACTIVATE, WM_NCCALCSIZE, WM_NCPAINT, WM_STYLECHANGING,
                    WS_BORDER, WS_CHILD, WS_DLGFRAME, WS_EX_NOACTIVATE, WS_MAXIMIZEBOX,
                    WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
                },
            },
        },
        core::{BOOL, w},
    };

    const NON_CLIENT_STYLE_MASK: u32 = WS_THICKFRAME.0
        | WS_BORDER.0
        | WS_DLGFRAME.0
        | WS_MINIMIZEBOX.0
        | WS_MAXIMIZEBOX.0
        | WS_SYSMENU.0;
    const FRAME_EX_STYLE_MASK: u32 = 0x0000_0001 // WS_EX_DLGMODALFRAME
        | 0x0000_0100 // WS_EX_WINDOWEDGE
        | 0x0000_0200 // WS_EX_CLIENTEDGE
        | 0x0002_0000; // WS_EX_STATICEDGE
    const FRAMELESS_SUBCLASS_ID: usize = 0x434F_4445_5857_4447;
    static DESKTOP_CHILD: AtomicBool = AtomicBool::new(false);

    #[derive(Clone, Copy)]
    pub struct NativeDragOrigin {
        cursor: POINT,
        window_x: i32,
        window_y: i32,
    }

    fn frameless_style(current_style: u32, attached: bool) -> u32 {
        let style = current_style & !NON_CLIENT_STYLE_MASK & !WS_CHILD.0 & !WS_POPUP.0;
        if attached {
            style | WS_CHILD.0
        } else {
            style | WS_POPUP.0
        }
    }

    fn frameless_ex_style(current_style: u32, attached: bool) -> u32 {
        let style = current_style & !FRAME_EX_STYLE_MASK & !WS_EX_NOACTIVATE.0;
        if attached {
            style | WS_EX_NOACTIVATE.0
        } else {
            style
        }
    }

    unsafe extern "system" fn frameless_subclass_proc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _subclass_id: usize,
        _reference_data: usize,
    ) -> LRESULT {
        unsafe {
            match message {
                // Intercept style restoration synchronously. Repairing this in the
                // next egui frame is one compositor frame too late and allows the
                // classic Windows caption to flash during focus/blur transitions.
                WM_STYLECHANGING if !lparam.0.is_negative() && lparam.0 != 0 => {
                    let styles = &mut *(lparam.0 as *mut STYLESTRUCT);
                    let style_index = wparam.0 as i32;
                    let attached = DESKTOP_CHILD.load(Ordering::Acquire);
                    if style_index == GWL_STYLE.0 {
                        styles.styleNew = frameless_style(styles.styleNew, attached);
                    } else if style_index == GWL_EXSTYLE.0 {
                        styles.styleNew = frameless_ex_style(styles.styleNew, attached);
                    }
                    DefSubclassProc(window, message, wparam, lparam)
                }
                // This is a fully client-drawn popup. Never allow activation to
                // create or repaint a native non-client area around it.
                WM_NCCALCSIZE => LRESULT(0),
                WM_NCPAINT => LRESULT(0),
                WM_NCACTIVATE => DefSubclassProc(window, message, wparam, LPARAM(-1)),
                // Avoid a GDI background clear becoming visible while Glow swaps
                // surfaces on an activation transition.
                WM_ERASEBKGND => LRESULT(1),
                _ => DefSubclassProc(window, message, wparam, lparam),
            }
        }
    }

    unsafe fn install_frameless_subclass(window: HWND, attached: bool) {
        unsafe {
            DESKTOP_CHILD.store(attached, Ordering::Release);
            let _ = SetWindowSubclass(
                window,
                Some(frameless_subclass_proc),
                FRAMELESS_SUBCLASS_ID,
                0,
            );
        }
    }

    fn hwnd(frame: &eframe::Frame) -> anyhow::Result<HWND> {
        let handle = frame
            .window_handle()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        match handle.as_raw() {
            RawWindowHandle::Win32(handle) => Ok(HWND(handle.hwnd.get() as *mut core::ffi::c_void)),
            _ => Err(anyhow::anyhow!("not a Win32 window")),
        }
    }

    unsafe extern "system" fn find_desktop_host_callback(top: HWND, data: LPARAM) -> BOOL {
        unsafe {
            let shell_view = FindWindowExW(Some(top), None, w!("SHELLDLL_DefView"), None);
            if shell_view.is_ok_and(|window| !window.0.is_null()) {
                // Use the Explorer host that contains the desktop icon view.
                // The separate wallpaper WorkerW is below that view and would
                // prevent this interactive widget from receiving pointer input.
                *(data.0 as *mut HWND) = top;
                return BOOL(0);
            }
            BOOL(1)
        }
    }

    fn desktop_host() -> anyhow::Result<HWND> {
        unsafe {
            let progman = FindWindowW(w!("Progman"), None)?;
            let mut message_result = 0_usize;
            let _ = SendMessageTimeoutW(
                progman,
                0x052C,
                WPARAM(0xD),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                Some(&mut message_result),
            );
            let _ = SendMessageTimeoutW(
                progman,
                0x052C,
                WPARAM(0xD),
                LPARAM(1),
                SMTO_NORMAL,
                1000,
                Some(&mut message_result),
            );
            let mut host = HWND::default();
            let _ = EnumWindows(
                Some(find_desktop_host_callback),
                LPARAM(&mut host as *mut HWND as isize),
            );
            if host.0.is_null() {
                Ok(progman)
            } else {
                Ok(host)
            }
        }
    }

    unsafe fn suppress_window_chrome(window: HWND, attached: bool) {
        unsafe {
            DESKTOP_CHILD.store(attached, Ordering::Release);
            install_frameless_subclass(window, attached);
            let current_style = GetWindowLongPtrW(window, GWL_STYLE) as u32;
            let frameless_style = frameless_style(current_style, attached);

            // Winit may restore these non-client edge bits after a viewport or
            // ownership update. WS_EX_WINDOWEDGE produces the bright top line
            // and white fragments around the native rounded region.
            let current_ex_style = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
            let frameless_ex_style = frameless_ex_style(current_ex_style, attached);

            let style_changed =
                current_style != frameless_style || current_ex_style != frameless_ex_style;
            if current_style != frameless_style {
                SetWindowLongPtrW(window, GWL_STYLE, frameless_style as isize);
            }
            if current_ex_style != frameless_ex_style {
                SetWindowLongPtrW(window, GWL_EXSTYLE, frameless_ex_style as isize);
            }
            if style_changed {
                let _ = SetWindowPos(
                    window,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }

            // The widget owns its shape through SetWindowRgn. DWM non-client
            // rendering would add a separate one-pixel active/inactive border,
            // most visibly across the top edge, so disable that second renderer.
            let nc_policy = DWMNCRP_DISABLED;
            let _ = DwmSetWindowAttribute(
                window,
                DWMWA_NCRENDERING_POLICY,
                &nc_policy as *const _ as *const core::ffi::c_void,
                size_of_val(&nc_policy) as u32,
            );

            // Do not let Windows 11 apply another rounded frame around the HRGN.
            let corner_preference = DWMWCP_DONOTROUND;
            let _ = DwmSetWindowAttribute(
                window,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner_preference as *const _ as *const core::ffi::c_void,
                size_of_val(&corner_preference) as u32,
            );

            // COLOR_NONE is documented as 0xFFFFFFFE. Keep this fallback for
            // systems that ignore the non-client rendering policy selectively.
            let border_color = 0xFFFF_FFFEu32;
            let _ = DwmSetWindowAttribute(
                window,
                DWMWA_BORDER_COLOR,
                &border_color as *const _ as *const core::ffi::c_void,
                size_of_val(&border_color) as u32,
            );
        }
    }

    pub fn set_attached(frame: &eframe::Frame, attached: bool) -> anyhow::Result<()> {
        unsafe {
            let window = hwnd(frame)?;
            let mut window_rect = RECT::default();
            GetWindowRect(window, &mut window_rect)?;

            if attached {
                let host = desktop_host()?;
                let mut position = POINT {
                    x: window_rect.left,
                    y: window_rect.top,
                };
                if !ScreenToClient(host, &mut position).as_bool() {
                    return Err(anyhow::anyhow!("ScreenToClient failed"));
                }

                // A child HWND is part of Explorer's desktop layer instead of
                // the top-level window stack. Keep it above SHELLDLL_DefView so
                // the weather controls remain interactive; every normal app is
                // still above the entire desktop host.
                install_frameless_subclass(window, true);
                let current_style = GetWindowLongPtrW(window, GWL_STYLE) as u32;
                SetWindowLongPtrW(
                    window,
                    GWL_STYLE,
                    frameless_style(current_style, true) as isize,
                );
                // SetParent returns the *previous* parent. A null return can
                // therefore mean either failure or a successful first attach
                // from a top-level window. The windows crate maps both cases to
                // Err, so confirm the resulting parent before rejecting it.
                if let Err(error) = SetParent(window, Some(host))
                    && !GetParent(window).is_ok_and(|parent| parent == host)
                {
                    return Err(error.into());
                }
                SetWindowPos(
                    window,
                    Some(HWND_TOP),
                    position.x,
                    position.y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                )?;
            } else {
                install_frameless_subclass(window, false);
                SetParent(window, None)?;
                let current_style = GetWindowLongPtrW(window, GWL_STYLE) as u32;
                SetWindowLongPtrW(
                    window,
                    GWL_STYLE,
                    frameless_style(current_style, false) as isize,
                );
                SetWindowPos(
                    window,
                    None,
                    window_rect.left,
                    window_rect.top,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                )?;
            }
            suppress_window_chrome(window, attached);
        }
        Ok(())
    }

    pub fn is_desktop_attached(frame: &eframe::Frame) -> bool {
        let Ok(window) = hwnd(frame) else {
            return false;
        };
        unsafe {
            let style = GetWindowLongPtrW(window, GWL_STYLE) as u32;
            if style & WS_CHILD.0 == 0 {
                return false;
            }
            GetParent(window)
                .is_ok_and(|parent| !parent.0.is_null() && IsWindow(Some(parent)).as_bool())
        }
    }

    pub fn request_keyboard_focus(frame: &eframe::Frame) -> bool {
        let Ok(window) = hwnd(frame) else {
            return false;
        };
        unsafe { SetForegroundWindow(window).as_bool() }
    }

    pub fn begin_window_drag(frame: &eframe::Frame) -> Option<NativeDragOrigin> {
        let window = hwnd(frame).ok()?;
        let mut cursor = POINT::default();
        let mut rect = RECT::default();
        unsafe {
            if GetCursorPos(&mut cursor).is_err() || GetWindowRect(window, &mut rect).is_err() {
                return None;
            }
        }
        Some(NativeDragOrigin {
            cursor,
            window_x: rect.left,
            window_y: rect.top,
        })
    }

    pub fn drag_window_to(frame: &eframe::Frame, origin: NativeDragOrigin) -> Option<(i32, i32)> {
        let window = hwnd(frame).ok()?;
        let mut cursor = POINT::default();
        if unsafe { GetCursorPos(&mut cursor).is_err() } {
            return None;
        }
        let screen_x = origin.window_x + cursor.x - origin.cursor.x;
        let screen_y = origin.window_y + cursor.y - origin.cursor.y;
        let mut parent_position = POINT {
            x: screen_x,
            y: screen_y,
        };
        unsafe {
            let style = GetWindowLongPtrW(window, GWL_STYLE) as u32;
            if style & WS_CHILD.0 != 0 {
                let parent = GetParent(window).ok()?;
                if !ScreenToClient(parent, &mut parent_position).as_bool() {
                    return None;
                }
            }
            SetWindowPos(
                window,
                None,
                parent_position.x,
                parent_position.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS,
            )
            .ok()?;
        }
        Some((screen_x, screen_y))
    }

    pub fn suppress_native_chrome(frame: &eframe::Frame, attached: bool) -> anyhow::Result<()> {
        let window = hwnd(frame)?;
        unsafe { suppress_window_chrome(window, attached) };
        Ok(())
    }

    pub fn wallpaper_path() -> anyhow::Result<Option<PathBuf>> {
        let mut buffer = vec![0_u16; 32_768];
        unsafe {
            SystemParametersInfoW(
                SPI_GETDESKWALLPAPER,
                buffer.len() as u32,
                Some(buffer.as_mut_ptr().cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )?;
        }
        let length = buffer
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(buffer.len());
        if length == 0 {
            return Ok(None);
        }
        Ok(Some(PathBuf::from(String::from_utf16_lossy(
            &buffer[..length],
        ))))
    }

    pub fn desktop_sample(frame: &eframe::Frame) -> Option<DesktopSample> {
        let window = hwnd(frame).ok()?;
        let mut window_rect = RECT::default();
        if unsafe { GetWindowRect(window, &mut window_rect).is_err() } {
            return None;
        }
        let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
        let mut monitor_info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !unsafe { GetMonitorInfoW(monitor, &mut monitor_info) }.as_bool() {
            return None;
        }
        let monitor_rect = monitor_info.rcMonitor;
        let width = (monitor_rect.right - monitor_rect.left) as f32;
        let height = (monitor_rect.bottom - monitor_rect.top) as f32;
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        Some(DesktopSample {
            monitor_size: [width, height],
            window_fraction: [
                ((window_rect.left - monitor_rect.left) as f32 / width).clamp(0.0, 1.0),
                ((window_rect.top - monitor_rect.top) as f32 / height).clamp(0.0, 1.0),
                ((window_rect.right - monitor_rect.left) as f32 / width).clamp(0.0, 1.0),
                ((window_rect.bottom - monitor_rect.top) as f32 / height).clamp(0.0, 1.0),
            ],
        })
    }

    pub fn apply_round_region(frame: &eframe::Frame, radius: i32) -> anyhow::Result<(u32, u32)> {
        let window = frame
            .winit_window()
            .ok_or_else(|| anyhow::anyhow!("window is not ready"))?;
        let size = window.inner_size();
        unsafe {
            let window_handle = hwnd(frame)?;
            // The compositor's rounded-corner preference is not a hard clip for the
            // client surface. A full-window effect (for example the lightning flash)
            // can otherwise reveal the rectangular swapchain for one frame. Keep a
            // native HRGN installed on every supported Windows version as the final
            // clipping boundary, then let DWM smooth the visible edge on Windows 11.
            // WorkerW composition can leave the first swapchain scanline outside normal
            // scene rendering on some Windows systems. Exclude that one row
            // from the native window region so the desktop shows through it;
            // y=1, which is fully textured, becomes the first visible row.
            let region =
                CreateRoundRectRgn(0, 1, size.width as i32, size.height as i32, radius, radius);
            if region.is_invalid() {
                return Err(anyhow::anyhow!("CreateRoundRectRgn failed"));
            }
            if SetWindowRgn(window_handle, Some(region), true) == 0 {
                return Err(anyhow::anyhow!("SetWindowRgn failed"));
            }

            let attached = GetWindowLongPtrW(window_handle, GWL_STYLE) as u32 & WS_CHILD.0 != 0;
            suppress_window_chrome(window_handle, attached);
        }
        Ok((size.width, size.height))
    }

    pub fn has_window_region(frame: &eframe::Frame) -> bool {
        let Ok(window_handle) = hwnd(frame) else {
            return false;
        };
        let Some(window) = frame.winit_window() else {
            return false;
        };
        let size = window.inner_size();
        let mut bounds = RECT::default();
        let region_kind = unsafe { GetWindowRgnBox(window_handle, &mut bounds) };
        region_kind != RGN_ERROR
            && bounds.left == 0
            && bounds.top == 1
            && bounds.right == size.width.saturating_sub(1) as i32
            && bounds.bottom == size.height.saturating_sub(1) as i32
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use std::path::PathBuf;

    use super::DesktopSample;
    #[derive(Clone, Copy)]
    pub struct NativeDragOrigin;

    pub fn set_attached(_frame: &eframe::Frame, _attached: bool) -> anyhow::Result<()> {
        Ok(())
    }
    pub fn apply_round_region(_frame: &eframe::Frame, _radius: i32) -> anyhow::Result<(u32, u32)> {
        Ok((0, 0))
    }
    pub fn has_window_region(_frame: &eframe::Frame) -> bool {
        true
    }
    pub fn is_desktop_attached(_frame: &eframe::Frame) -> bool {
        false
    }
    pub fn request_keyboard_focus(_frame: &eframe::Frame) -> bool {
        true
    }
    pub fn begin_window_drag(_frame: &eframe::Frame) -> Option<NativeDragOrigin> {
        None
    }
    pub fn drag_window_to(_frame: &eframe::Frame, _origin: NativeDragOrigin) -> Option<(i32, i32)> {
        None
    }
    pub fn suppress_native_chrome(_frame: &eframe::Frame, _attached: bool) -> anyhow::Result<()> {
        Ok(())
    }

    pub fn wallpaper_path() -> anyhow::Result<Option<PathBuf>> {
        Ok(None)
    }

    pub fn desktop_sample(_frame: &eframe::Frame) -> Option<DesktopSample> {
        None
    }
}

pub use platform::{
    NativeDragOrigin, apply_round_region, begin_window_drag, desktop_sample, drag_window_to,
    has_window_region, is_desktop_attached, request_keyboard_focus, set_attached,
    suppress_native_chrome, wallpaper_path,
};

//! Let Windows own the outer outline, including resize and maximize behavior.

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        DwmSetWindowAttribute,
    },
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            IsIconic, IsZoomed, NCCALCSIZE_PARAMS, PostMessageW, SWP_FRAMECHANGED, SWP_NOACTIVATE,
            SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos, WM_APP, WM_NCCALCSIZE,
            WM_NCDESTROY,
        },
    },
};

const FRAME_SUBCLASS: usize = 1;
const REFRESH_FRAME: u32 = WM_APP + 0x43b;

pub fn configure(window: &Window) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = handle.hwnd.get() as HWND;
    let corners = DWMWCP_ROUND;
    let border = DWMWA_COLOR_NONE;
    // Windows 11 rounds its compositor surface and controls its outline. Older
    // Windows can reject these cosmetic hints; a full rectangular client fill
    // still gives a clean frame there. Do not apply a separate window region.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&corners as *const i32).cast(),
            std::mem::size_of_val(&corners) as u32,
        );
        let border_result = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR as u32,
            (&border as *const u32).cast(),
            std::mem::size_of_val(&border) as u32,
        );
        // GPUI 0.2.2 reserves a thin top strip for the Windows 11 frame.
        // Once its border is suppressed, let the client paint that strip too.
        // Installing on this window's UI thread preserves GPUI's message chain.
        if border_result >= 0
            && SetWindowSubclass(hwnd, Some(frame_callback), FRAME_SUBCLASS, 0) != 0
        {
            // open_window is still registering its root view. Wait for the
            // message loop so WM_SIZE can update GPUI's viewport normally.
            let _ = PostMessageW(hwnd, REFRESH_FRAME, 0, 0);
        }
    }
}

unsafe extern "system" fn frame_callback(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    subclass: usize,
    _: usize,
) -> LRESULT {
    if message == REFRESH_FRAME {
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
        return 0;
    }
    if message == WM_NCDESTROY {
        // No allocated state is attached to this callback. Remove it before
        // forwarding the final message to GPUI's original window procedure.
        unsafe { RemoveWindowSubclass(hwnd, Some(frame_callback), subclass) };
    }
    if message == WM_NCCALCSIZE && wparam != 0 && lparam != 0 {
        // Copy before forwarding; do not hold a Rust reference while GPUI
        // modifies the same NCCALCSIZE_PARAMS through the subclass chain.
        let requested = unsafe { (*(lparam as *const NCCALCSIZE_PARAMS)).rgrc[0] };
        let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        if result == 0 && unsafe { IsZoomed(hwnd) == 0 && IsIconic(hwnd) == 0 } {
            let client = unsafe { &mut (*(lparam as *mut NCCALCSIZE_PARAMS)).rgrc[0] };
            let top = client.top - requested.top;
            let left = client.left - requested.left;
            let right = requested.right - client.right;
            // Only remove the small restored-window top inset. The larger
            // maximized inset, resize sides, and taskbar handling stay in GPUI.
            if top > 0 && top < left && top < right {
                client.top = requested.top;
            }
        }
        return result;
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

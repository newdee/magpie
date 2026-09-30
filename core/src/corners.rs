//! Rounded palette corners on Windows (#16). The acrylic backdrop fills the
//! whole rectangular window, so around the panel's CSS-rounded corners a
//! square of frosted glass showed. Windows 11 can round the window itself;
//! Windows 10 cannot, and there the panel goes square to match.

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DWM_WINDOW_CORNER_PREFERENCE,
};

/// The radius Windows 11 gives a rounded window, in logical pixels; the
/// panel's CSS radius follows it so the two outlines coincide.
pub const WINDOWS_RADIUS: u32 = 8;

/// Ask DWM for rounded corners and no border line on `hwnd`. True when the
/// system took it (Windows 11); false where the attribute does not exist.
pub fn round(hwnd: isize) -> bool {
    let h = HWND(hwnd as *mut core::ffi::c_void);
    let pref = DWMWCP_ROUND;
    // SAFETY: a pointer to a local of the size DWM expects for this attribute
    let set = unsafe {
        DwmSetWindowAttribute(h, DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const _ as *const _, size_of_val(&pref) as u32)
    };
    if set.is_err() {
        return false;
    }
    // DWMWA_COLOR_NONE: no accent line along the rounded edge
    let none: u32 = 0xFFFF_FFFE;
    // SAFETY: as above
    let _ = unsafe { DwmSetWindowAttribute(h, DWMWA_BORDER_COLOR, &none as *const _ as *const _, size_of_val(&none) as u32) };
    corner_preference(hwnd) == Some(DWMWCP_ROUND)
}

/// What DWM holds for `hwnd` now (for tests and the log).
pub fn corner_preference(hwnd: isize) -> Option<DWM_WINDOW_CORNER_PREFERENCE> {
    let h = HWND(hwnd as *mut core::ffi::c_void);
    let mut pref = DWM_WINDOW_CORNER_PREFERENCE(0);
    // SAFETY: DWM writes at most size_of(pref) bytes into the local
    unsafe { DwmGetWindowAttribute(h, DWMWA_WINDOW_CORNER_PREFERENCE, &mut pref as *mut _ as *mut _, size_of_val(&pref) as u32) }
        .ok()
        .map(|_| pref)
}

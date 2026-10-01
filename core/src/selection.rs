//! What the selection search (#17) needs from the system around its
//! synthesized copy chord: whether the user still holds the chord's
//! modifiers, a clipboard change counter, and on macOS whether magpie may
//! send keystrokes at all.
//!
//! The copy chord goes out while the user may still be holding the selection
//! hotkey (Option+Shift+Space on macOS). The front app then sees ⌘ plus those
//! modifiers, ⌥⇧⌘C, which is not Copy, and nothing was copied. The caller
//! waits for [`modifiers_held`] to clear first.

/// Is any of Shift, Control, Alt/Option or Win/Command down on the keyboard
/// right now? Linux has no way to ask that fits here and says false, so it
/// sends at once, as before.
pub fn modifiers_held() -> bool {
    imp::modifiers_held()
}

/// A counter the system bumps on every clipboard write, when it keeps one
/// (Windows, macOS). With it a copy of the text already on the clipboard
/// still counts as a copy; comparing the text alone reads that as "nothing
/// was selected".
pub fn clipboard_seq() -> Option<u64> {
    imp::clipboard_seq()
}

/// May magpie send keystrokes to other apps? macOS drops them silently
/// without the Accessibility permission; elsewhere nothing gates them.
pub fn input_allowed() -> bool {
    imp::input_allowed()
}

/// The query a selection search runs, from the clipboard before and after
/// the copy chord. A change counter, when both reads have one, decides
/// whether the chord copied anything; without one, a changed text does.
/// Nothing copied means an empty query, never the stale clip.
pub fn copied_query(
    seq: (Option<u64>, Option<u64>),
    before: Option<&str>,
    after: Option<&str>,
) -> String {
    let copied = match seq {
        (Some(b), Some(a)) => a != b,
        _ => after.is_some() && after != before,
    };
    match after {
        Some(a) if copied => a.trim().chars().take(500).collect(),
        _ => String::new(),
    }
}

#[cfg(windows)]
mod imp {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    pub fn modifiers_held() -> bool {
        [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
            .iter()
            // SAFETY: reads the async key state; no pointers involved.
            // The high bit (a negative value) means the key is down now
            .any(|k| unsafe { GetAsyncKeyState(k.0 as i32) } < 0)
    }

    pub fn clipboard_seq() -> Option<u64> {
        clipboard_win::seq_num().map(|n| u64::from(n.get()))
    }

    pub fn input_allowed() -> bool {
        true
    }
}

#[cfg(target_os = "macos")]
mod imp {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceFlagsState(state_id: i32) -> u64;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> u8;
    }

    /// kCGEventSourceStateHIDSystemState: the keys as the hardware has them
    const HID_SYSTEM_STATE: i32 = 1;
    /// kCGEventFlagMask{Shift, Control, Alternate, Command}
    const MODIFIER_MASK: u64 = 0x0002_0000 | 0x0004_0000 | 0x0008_0000 | 0x0010_0000;

    pub fn modifiers_held() -> bool {
        // SAFETY: a plain query of the event system's flags
        let flags = unsafe { CGEventSourceFlagsState(HID_SYSTEM_STATE) };
        flags & MODIFIER_MASK != 0
    }

    pub fn clipboard_seq() -> Option<u64> {
        objc2::rc::autoreleasepool(|_| {
            let n = objc2_app_kit::NSPasteboard::generalPasteboard().changeCount();
            u64::try_from(n).ok()
        })
    }

    pub fn input_allowed() -> bool {
        // SAFETY: no arguments
        let trusted = unsafe { AXIsProcessTrusted() };
        trusted != 0
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    pub fn modifiers_held() -> bool {
        false
    }

    pub fn clipboard_seq() -> Option<u64> {
        None
    }

    pub fn input_allowed() -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::copied_query;

    #[test]
    fn a_counter_decides_when_both_reads_have_one() {
        // the selection was already on the clipboard: same text, new write
        assert_eq!(copied_query((Some(7), Some(8)), Some("same"), Some("same")), "same");
        // counter unchanged: the chord copied nothing, whatever the text says
        assert_eq!(copied_query((Some(7), Some(7)), Some("old"), Some("old")), "");
        // copied, but not text (an image): nothing to search
        assert_eq!(copied_query((Some(7), Some(8)), Some("old"), None), "");
    }

    #[test]
    fn without_a_counter_a_changed_text_decides() {
        assert_eq!(copied_query((None, None), Some("old"), Some("  new text \n")), "new text");
        assert_eq!(copied_query((None, None), Some("old"), Some("old")), "");
        assert_eq!(copied_query((None, None), None, Some("first")), "first");
        assert_eq!(copied_query((None, None), None, None), "");
        // one read without a counter falls back to the text
        assert_eq!(copied_query((Some(1), None), Some("a"), Some("b")), "b");
    }

    #[test]
    fn long_selections_are_cut_to_500_characters() {
        let long = "字".repeat(600);
        assert_eq!(copied_query((Some(1), Some(2)), None, Some(&long)).chars().count(), 500);
    }
}

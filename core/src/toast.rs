//! A Windows toast whose click reaches the app. The notification plugin (and
//! the crate under it) let go of the toast object as soon as it is shown, and
//! the click handler goes with it; here the latest toasts stay referenced, so
//! a click on the banner, or later in the notification centre, still arrives.

use std::sync::{Arc, Mutex};
use windows::core::HSTRING;
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

/// Toasts still on screen or in the notification centre (a few; older ones
/// have long been acted on or cleared).
static LIVE: Mutex<Vec<ToastNotification>> = Mutex::new(Vec::new());
const KEEP: usize = 8;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The toast XML: a title and lines under it, shown for the long duration
/// (about 25 s) so a banner is not gone before it is read.
fn xml(title: &str, lines: &[&str]) -> String {
    let mut texts = format!("<text>{}</text>", escape(title));
    for l in lines {
        texts.push_str(&format!("<text>{}</text>", escape(l)));
    }
    format!(r#"<toast duration="long"><visual><binding template="ToastGeneric">{texts}</binding></visual></toast>"#)
}

/// Show a toast as `app_id`; `on_click` runs when the toast is clicked.
pub fn show(app_id: &str, title: &str, lines: &[&str], on_click: impl Fn() + Send + Sync + 'static) -> anyhow::Result<()> {
    // WinRT needs COM on this thread; already initialised is fine
    // SAFETY: plain initialisation call, no pointers
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml(title, lines)))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    let on_click = Arc::new(on_click);
    toast.Activated(&TypedEventHandler::new(move |_, _| {
        on_click();
        Ok(())
    }))?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))?.Show(&toast)?;
    let mut live = LIVE.lock().unwrap_or_else(|p| p.into_inner());
    live.push(toast);
    if live.len() > KEEP {
        live.remove(0);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_xml_escapes_and_asks_for_the_long_duration() {
        let x = xml("今日寒露", &["a < b & c", "《池上》"]);
        assert!(x.starts_with(r#"<toast duration="long">"#));
        assert!(x.contains("<text>今日寒露</text><text>a &lt; b &amp; c</text><text>《池上》</text>"));
        let doc = XmlDocument::new().unwrap();
        doc.LoadXml(&HSTRING::from(x)).expect("well-formed");
    }
}

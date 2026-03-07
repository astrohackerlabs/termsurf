#[allow(deprecated)]
use objc2_app_kit::{NSFilenamesPboardType, NSPasteboardTypeString, NSStringPboardType};

use crate::macos::nsstring_to_str;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSPasteboard;
use objc2_foundation::NSString;

pub struct Clipboard {
    pasteboard: Retained<NSPasteboard>,
}

impl Clipboard {
    pub fn new() -> Self {
        let pasteboard = NSPasteboard::generalPasteboard();
        Clipboard { pasteboard }
    }

    pub fn read(&self) -> anyhow::Result<String> {
        unsafe {
            #[allow(deprecated)]
            let plist = self.pasteboard.propertyListForType(NSFilenamesPboardType);
            if let Some(plist) = plist {
                let mut filenames = vec![];
                let count: isize = objc2::msg_send![&*plist, count];
                for i in 0..count {
                    let obj: *mut AnyObject = objc2::msg_send![&*plist, objectAtIndex: i];
                    filenames
                        .push(shlex::try_quote(nsstring_to_str(obj)).unwrap_or_else(|_| "".into()));
                }
                return Ok(filenames.join(" "));
            }
            #[allow(deprecated)]
            let s = self.pasteboard.stringForType(NSStringPboardType);
            if let Some(s) = s {
                let str = nsstring_to_str(Retained::as_ptr(&s) as *mut AnyObject);
                return Ok(str.to_string());
            }
        }
        anyhow::bail!("pasteboard read returned empty");
    }

    pub fn write(&mut self, data: String) -> anyhow::Result<()> {
        unsafe {
            self.pasteboard.clearContents();
            let ns_str = NSString::from_str(&data);
            let success = self
                .pasteboard
                .setString_forType(&ns_str, NSPasteboardTypeString);
            anyhow::ensure!(success, "pasteboard write returned false");
            Ok(())
        }
    }
}

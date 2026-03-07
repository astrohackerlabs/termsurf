use crate::connection::ConnectionOps;
use crate::macos::menu::RepresentedItem;
use crate::macos::{nsstring, nsstring_to_str};
use crate::menu::{Menu, MenuItem};
use crate::{ApplicationEvent, Connection};
use config::keyassignment::KeyAssignment;
use config::WindowCloseConfirmation;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2_app_kit::NSApplicationTerminateReply;

extern "C" fn application_should_terminate(
    _self: *mut AnyObject,
    _sel: Sel,
    _app: *mut AnyObject,
) -> u64 {
    log::debug!("application termination requested");
    unsafe {
        match config::configuration().window_close_confirmation {
            WindowCloseConfirmation::NeverPrompt => terminate_now(),
            WindowCloseConfirmation::AlwaysPrompt => {
                let ns_alert_cls = AnyClass::get(c"NSAlert").unwrap();
                let alert: *mut AnyObject = objc2::msg_send![ns_alert_cls, alloc];
                let alert: *mut AnyObject = objc2::msg_send![alert, init];
                let message_text = nsstring("Terminate Wezboard?");
                let info_text = nsstring("Detach and close all panes and terminate wezboard?");
                let cancel = nsstring("Cancel");
                let ok = nsstring("Ok");

                let () = objc2::msg_send![alert, setMessageText: Retained::as_ptr(&message_text)];
                let () = objc2::msg_send![alert, setInformativeText: Retained::as_ptr(&info_text)];
                let () = objc2::msg_send![alert, addButtonWithTitle: Retained::as_ptr(&cancel)];
                let () = objc2::msg_send![alert, addButtonWithTitle: Retained::as_ptr(&ok)];
                #[allow(non_upper_case_globals)]
                const NSModalResponseCancel: i64 = 1000;
                #[allow(non_upper_case_globals, dead_code)]
                const NSModalResponseOK: i64 = 1001;
                let result: i64 = objc2::msg_send![alert, runModal];
                log::info!("alert result is {result}");

                if result == NSModalResponseCancel {
                    NSApplicationTerminateReply::TerminateCancel.0 as u64
                } else {
                    terminate_now()
                }
            }
        }
    }
}

fn terminate_now() -> u64 {
    if let Some(conn) = Connection::get() {
        conn.terminate_message_loop();
    }
    NSApplicationTerminateReply::TerminateNow.0 as u64
}

extern "C" fn application_will_finish_launching(
    _self: *mut AnyObject,
    _sel: Sel,
    _notif: *mut AnyObject,
) {
    log::debug!("application_will_finish_launching");
}

extern "C" fn application_did_finish_launching(
    this: *mut AnyObject,
    _sel: Sel,
    _notif: *mut AnyObject,
) {
    log::debug!("application_did_finish_launching");
    #[allow(deprecated)]
    unsafe {
        *(&mut *this).get_mut_ivar::<Bool>("launched") = Bool::YES;
    }
}

extern "C" fn application_open_untitled_file(
    this: *mut AnyObject,
    _sel: Sel,
    _app: *mut AnyObject,
) -> Bool {
    #[allow(deprecated)]
    let launched: Bool = unsafe { *(&*this).get_ivar("launched") };
    log::debug!("application_open_untitled_file launched={launched:?}");
    if let Some(conn) = Connection::get() {
        if launched.as_bool() {
            conn.dispatch_app_event(ApplicationEvent::PerformKeyAssignment(
                KeyAssignment::SpawnWindow,
            ));
        }
        return Bool::YES;
    }
    Bool::NO
}

extern "C" fn wezboard_perform_key_assignment(
    _self: *mut AnyObject,
    _sel: Sel,
    menu_item: *mut AnyObject,
) {
    let menu_item = crate::os::macos::menu::MenuItem::with_menu_item(menu_item.cast());
    // Safe because wezboardPerformKeyAssignment: is only used with KeyAssignment
    let action = menu_item.get_represented_item();
    log::debug!("wezboard_perform_key_assignment {action:?}",);
    match action {
        Some(RepresentedItem::KeyAssignment(action)) => {
            if let Some(conn) = Connection::get() {
                conn.dispatch_app_event(ApplicationEvent::PerformKeyAssignment(action));
            }
        }
        None => {}
    }
}

extern "C" fn application_open_file(
    this: *mut AnyObject,
    _sel: Sel,
    _app: *mut AnyObject,
    file_name: *mut AnyObject,
) {
    #[allow(deprecated)]
    let launched: Bool = unsafe { *(&*this).get_ivar("launched") };
    if launched.as_bool() {
        let file_name = unsafe { nsstring_to_str(file_name) }.to_string();
        if let Some(conn) = Connection::get() {
            log::debug!("application_open_file {file_name}");
            conn.dispatch_app_event(ApplicationEvent::OpenCommandScript(file_name));
        }
    }
}

extern "C" fn application_dock_menu(
    _self: *mut AnyObject,
    _sel: Sel,
    _app: *mut AnyObject,
) -> *mut AnyObject {
    let dock_menu = Menu::new_with_title("");
    let new_window_item = MenuItem::new_with(
        "New Window",
        Some(objc2::sel!(wezboardPerformKeyAssignment:)),
        "",
    );
    new_window_item
        .set_represented_item(RepresentedItem::KeyAssignment(KeyAssignment::SpawnWindow));
    dock_menu.add_item(&new_window_item);
    dock_menu.autorelease()
}

fn get_class() -> &'static AnyClass {
    AnyClass::get(c"WezboardAppDelegate").unwrap_or_else(|| {
        let mut cls =
            ClassBuilder::new(c"WezboardAppDelegate", AnyClass::get(c"NSObject").unwrap())
                .expect("Unable to register application delegate class");

        cls.add_ivar::<Bool>(c"launched");

        unsafe {
            cls.add_method(
                objc2::sel!(applicationShouldTerminate:),
                application_should_terminate
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> u64,
            );
            cls.add_method(
                objc2::sel!(applicationWillFinishLaunching:),
                application_will_finish_launching
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(applicationDidFinishLaunching:),
                application_did_finish_launching
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(application:openFile:),
                application_open_file
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(applicationDockMenu:),
                application_dock_menu
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> *mut AnyObject,
            );
            cls.add_method(
                objc2::sel!(wezboardPerformKeyAssignment:),
                wezboard_perform_key_assignment
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(applicationOpenUntitledFile:),
                application_open_untitled_file
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool,
            );
        }

        cls.register()
    })
}

pub fn create_app_delegate() -> anyhow::Result<Retained<AnyObject>> {
    let cls = get_class();
    unsafe {
        let delegate: *mut AnyObject = objc2::msg_send![cls, alloc];
        let delegate: *mut AnyObject = objc2::msg_send![delegate, init];
        // ObjC zeroes ivars on alloc, so `launched` is already false
        Retained::from_raw(delegate).ok_or_else(|| anyhow::anyhow!("AppDelegate init returned nil"))
    }
}

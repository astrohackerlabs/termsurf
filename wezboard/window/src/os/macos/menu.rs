use crate::macos::nsstring_to_str;
use config::keyassignment::KeyAssignment;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
pub use objc2_app_kit::NSEventModifierFlags;
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
use objc2_foundation::{MainThreadMarker, NSInteger, NSString};
use std::ffi::c_void;

pub struct Menu {
    menu: Retained<NSMenu>,
}

impl Menu {
    pub fn new_with_title(title: &str) -> Self {
        let mtm = MainThreadMarker::new().unwrap();
        let menu = NSMenu::initWithTitle(mtm.alloc::<NSMenu>(), &NSString::from_str(title));
        Self { menu }
    }

    pub fn autorelease(self) -> *mut AnyObject {
        let ptr = Retained::into_raw(self.menu) as *mut AnyObject;
        unsafe { objc2::msg_send![ptr, autorelease] }
    }

    pub fn item_at_index(&self, index: usize) -> Option<MenuItem> {
        let index = index as NSInteger;
        let item = self.menu.itemAtIndex(index);
        item.map(|item| MenuItem { item })
    }

    pub fn assign_as_main_menu(&self) {
        let mtm = MainThreadMarker::new().unwrap();
        let ns_app = NSApplication::sharedApplication(mtm);
        ns_app.setMainMenu(Some(&self.menu));
    }

    pub fn get_main_menu() -> Option<Self> {
        let mtm = MainThreadMarker::new().unwrap();
        let ns_app = NSApplication::sharedApplication(mtm);
        let menu = ns_app.mainMenu()?;
        Some(Self { menu })
    }

    pub fn assign_as_help_menu(&self) {
        let mtm = MainThreadMarker::new().unwrap();
        let ns_app = NSApplication::sharedApplication(mtm);
        ns_app.setHelpMenu(Some(&self.menu));
    }

    pub fn assign_as_windows_menu(&self) {
        let mtm = MainThreadMarker::new().unwrap();
        let ns_app = NSApplication::sharedApplication(mtm);
        ns_app.setWindowsMenu(Some(&self.menu));
    }

    pub fn assign_as_services_menu(&self) {
        let mtm = MainThreadMarker::new().unwrap();
        let ns_app = NSApplication::sharedApplication(mtm);
        ns_app.setServicesMenu(Some(&self.menu));
    }

    pub fn assign_as_app_menu(&self) {
        let mtm = MainThreadMarker::new().unwrap();
        let ns_app = NSApplication::sharedApplication(mtm);
        unsafe {
            let () = objc2::msg_send![&*ns_app, setAppleMenu: &*self.menu];
        }
    }

    pub fn add_item(&self, item: &MenuItem) {
        self.menu.addItem(&item.item);
    }

    pub fn item_with_title(&self, title: &str) -> Option<MenuItem> {
        unsafe {
            let item: *mut AnyObject = objc2::msg_send![
                &*self.menu,
                itemWithTitle: &*NSString::from_str(title)
            ];
            if item.is_null() {
                None
            } else {
                let item = Retained::retain(item as *mut NSMenuItem).unwrap();
                Some(MenuItem { item })
            }
        }
    }

    pub fn get_or_create_sub_menu<F: FnOnce(&Menu)>(&self, title: &str, on_create: F) -> Menu {
        match self.item_with_title(title) {
            Some(m) => m.get_sub_menu().unwrap(),
            None => {
                let item = MenuItem::new_with(title, None, "");
                let menu = Menu::new_with_title(title);
                item.set_sub_menu(&menu);
                self.add_item(&item);
                on_create(&menu);
                menu
            }
        }
    }

    pub fn get_sub_menu(&self, title: &str) -> Menu {
        self.item_with_title(title).unwrap().get_sub_menu().unwrap()
    }

    pub fn remove_all_items(&self) {
        self.menu.removeAllItems();
    }

    pub fn remove_item(&self, item: &MenuItem) {
        self.menu.removeItem(&item.item);
    }

    pub fn items(&self) -> Vec<MenuItem> {
        unsafe {
            let n: NSInteger = objc2::msg_send![&*self.menu, numberOfItems];
            let mut items = vec![];
            for i in 0..n {
                items.push(self.item_at_index(i as _).expect("index to be valid"));
            }
            items
        }
    }

    pub fn index_of_item_with_represented_object(&self, object: *mut AnyObject) -> Option<usize> {
        unsafe {
            let n: NSInteger = objc2::msg_send![
                &*self.menu,
                indexOfItemWithRepresentedObject: object
            ];
            if n == -1 { None } else { Some(n as usize) }
        }
    }

    pub fn index_of_item_with_represented_item(&self, item: &RepresentedItem) -> Option<usize> {
        let wrapped = item.clone().wrap();
        let ptr = Retained::as_ptr(&wrapped) as *mut AnyObject;
        self.index_of_item_with_represented_object(ptr)
    }

    pub fn get_item_with_represented_item(&self, item: &RepresentedItem) -> Option<MenuItem> {
        let idx = self.index_of_item_with_represented_item(item)?;
        self.item_at_index(idx)
    }
}

pub struct MenuItem {
    item: Retained<NSMenuItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RepresentedItem {
    KeyAssignment(KeyAssignment),
}

impl RepresentedItem {
    fn wrap(self) -> Retained<AnyObject> {
        let wrapper_cls =
            AnyClass::get(c"WezboardNSMenuRepresentedItem").unwrap_or_else(|| get_wrapper_class());
        let wrapper: *mut AnyObject = unsafe { objc2::msg_send![wrapper_cls, alloc] };
        let wrapper: *mut AnyObject = unsafe { objc2::msg_send![wrapper, init] };
        let item = Box::new(self);
        let item: *const RepresentedItem = Box::into_raw(item);
        let item = item as *const c_void;
        #[allow(deprecated)]
        unsafe {
            *(*wrapper).get_mut_ivar::<*const c_void>(WRAPPER_FIELD_NAME) = item;
            Retained::from_raw(wrapper).unwrap()
        }
    }

    unsafe fn ref_item(wrapper: *mut AnyObject) -> Option<RepresentedItem> {
        #[allow(deprecated)]
        let item = (*wrapper).get_ivar::<*const c_void>(WRAPPER_FIELD_NAME);
        let item = (*item) as *const RepresentedItem;
        if item.is_null() {
            None
        } else {
            Some((*item).clone())
        }
    }
}

impl MenuItem {
    pub fn with_menu_item(item: *mut AnyObject) -> Self {
        let item = item as *mut NSMenuItem;
        let item = unsafe { Retained::retain(item).unwrap() };
        Self { item }
    }

    pub fn new_separator() -> Self {
        let mtm = MainThreadMarker::new().unwrap();
        let item = NSMenuItem::separatorItem(mtm);
        Self { item }
    }

    pub fn new_with(title: &str, action: Option<Sel>, key: &str) -> Self {
        let mtm = MainThreadMarker::new().unwrap();
        unsafe {
            let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc::<NSMenuItem>(),
                &NSString::from_str(title),
                action,
                &NSString::from_str(key),
            );
            Self { item }
        }
    }

    pub fn get_action(&self) -> Option<Sel> {
        self.item.action()
    }

    pub fn set_tool_tip(&self, tip: &str) {
        self.item.setToolTip(Some(&NSString::from_str(tip)));
    }

    pub fn set_target(&self, target: *mut AnyObject) {
        unsafe {
            let () = objc2::msg_send![&*self.item, setTarget: target];
        }
    }

    pub fn set_sub_menu(&self, menu: &Menu) {
        self.item.setSubmenu(Some(&menu.menu));
    }

    pub fn get_sub_menu(&self) -> Option<Menu> {
        let menu = self.item.submenu()?;
        Some(Menu { menu })
    }

    pub fn get_parent_item(&self) -> Option<Self> {
        let item = unsafe { self.item.parentItem()? };
        Some(Self { item })
    }

    pub fn get_menu(&self) -> Option<Menu> {
        let menu = unsafe { self.item.menu()? };
        Some(Menu { menu })
    }

    /// Set an integer tag to identify this item
    pub fn set_tag(&self, tag: NSInteger) {
        self.item.setTag(tag);
    }

    pub fn get_title(&self) -> String {
        let title = self.item.title();
        let ptr = Retained::as_ptr(&title) as *mut AnyObject;
        unsafe { nsstring_to_str(ptr).to_string() }
    }

    pub fn set_title(&self, title: &str) {
        self.item.setTitle(&NSString::from_str(title));
    }

    pub fn set_key_equivalent(&self, equiv: &str) {
        self.item.setKeyEquivalent(&NSString::from_str(equiv));
    }

    pub fn get_tag(&self) -> NSInteger {
        self.item.tag()
    }

    /// Associate the item to an object
    fn set_represented_object(&self, object: *mut AnyObject) {
        unsafe {
            let () = objc2::msg_send![&*self.item, setRepresentedObject: object];
        }
    }

    fn get_represented_object(&self) -> Option<Retained<AnyObject>> {
        unsafe {
            let object: *mut AnyObject = objc2::msg_send![&*self.item, representedObject];
            if object.is_null() {
                None
            } else {
                Some(Retained::retain(object).unwrap())
            }
        }
    }

    pub fn set_represented_item(&self, item: RepresentedItem) {
        let wrapper = item.wrap();
        self.set_represented_object(Retained::as_ptr(&wrapper) as *mut AnyObject);
    }

    pub fn get_represented_item(&self) -> Option<RepresentedItem> {
        let wrapper = self.get_represented_object()?;
        unsafe { RepresentedItem::ref_item(Retained::as_ptr(&wrapper) as *mut AnyObject) }
    }

    pub fn set_key_equiv_modifier_mask(&self, mods: NSEventModifierFlags) {
        self.item.setKeyEquivalentModifierMask(mods);
    }
}

const WRAPPER_CLS_NAME: &std::ffi::CStr = c"WezboardNSMenuRepresentedItem";
const WRAPPER_FIELD_NAME: &str = "item";
const WRAPPER_FIELD_CNAME: &std::ffi::CStr = c"item";
/// Wraps RepresentedItem in an NSObject so that we can associate
/// it with a MenuItem
fn get_wrapper_class() -> &'static AnyClass {
    AnyClass::get(WRAPPER_CLS_NAME).unwrap_or_else(|| {
        let mut cls = ClassBuilder::new(WRAPPER_CLS_NAME, AnyClass::get(c"NSObject").unwrap())
            .expect("Unable to register class");

        extern "C" fn dealloc(this: *mut AnyObject, _sel: Sel) {
            unsafe {
                #[allow(deprecated)]
                let item = (*this).get_ivar::<*mut c_void>(WRAPPER_FIELD_NAME);
                let item = (*item) as *mut RepresentedItem;
                if item.is_null() {
                    return;
                }
                // SAFETY: pointer was created by Box::into_raw in RepresentedItem::new
                // and the null check above ensures we don't free a null pointer.
                let item = Box::from_raw(item);
                drop(item);
                let superclass = (*this).class().superclass().unwrap();
                let () = objc2::msg_send![super(this as *const AnyObject, superclass), dealloc];
            }
        }

        extern "C" fn is_equal(this: *mut AnyObject, _sel: Sel, that: *mut AnyObject) -> Bool {
            unsafe {
                let this_item = RepresentedItem::ref_item(this);
                let that_item = RepresentedItem::ref_item(that);
                if this_item == that_item {
                    Bool::YES
                } else {
                    Bool::NO
                }
            }
        }

        cls.add_ivar::<*mut c_void>(WRAPPER_FIELD_CNAME);
        unsafe {
            cls.add_method(
                objc2::sel!(dealloc),
                dealloc as extern "C" fn(*mut AnyObject, Sel),
            );
            cls.add_method(
                objc2::sel!(isEqual:),
                is_equal as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool,
            );
        }
        cls.register()
    })
}

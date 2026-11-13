//! macOS: the application object Chromium expects, and a main menu.
//!
//! Chromium asks the shared NSApplication whether it is in the middle of `sendEvent:` and
//! leaves its own run loop only when told to quit, so the application must be a subclass
//! that answers both. The menu carries the edit commands (copy, paste, …) that macOS routes
//! through menu key equivalents rather than to the focused web page directly.
use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool, NSObjectProtocol, Sel},
    sel,
};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSEvent, NSMenu, NSMenuItem};
use objc2_foundation::NSString;
use std::cell::Cell;

#[derive(Default)]
pub struct ShellApplicationIvars {
    handling_send_event: Cell<Bool>,
}

define_class!(
    #[unsafe(super(NSApplication))]
    #[ivars = ShellApplicationIvars]
    #[name = "ZurufShellApplication"]
    pub struct ShellApplication;

    impl ShellApplication {
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &NSEvent) {
            let handling = &self.ivars().handling_send_event;
            let outer = handling.replace(Bool::YES);
            // SAFETY: forwards the event unchanged to NSApplication's own implementation.
            let _: () = unsafe { msg_send![super(self), sendEvent: event] };
            handling.set(outer);
        }

        /// Quit from the menu, the Dock or logout: close the window so that Chromium shuts
        /// down in order instead of NSApplication calling exit().
        #[unsafe(method(terminate:))]
        fn terminate(&self, _sender: Option<&AnyObject>) {
            crate::shell::quit();
        }
    }

    unsafe impl CrAppProtocol for ShellApplication {
        #[unsafe(method(isHandlingSendEvent))]
        fn is_handling_send_event(&self) -> Bool {
            self.ivars().handling_send_event.get()
        }
    }

    unsafe impl CrAppControlProtocol for ShellApplication {
        #[unsafe(method(setHandlingSendEvent:))]
        fn set_handling_send_event(&self, handling_send_event: Bool) {
            self.ivars().handling_send_event.set(handling_send_event);
        }
    }

    unsafe impl CefAppProtocol for ShellApplication {}
);

/// Make `ShellApplication` the shared application. Must run on the main thread before
/// anything else touches NSApp, which would create a plain NSApplication instead.
pub fn prepare() -> Result<(), &'static str> {
    MainThreadMarker::new().ok_or("not on the main thread")?;
    // SAFETY: `sharedApplication` on the subclass creates the one shared instance.
    let app: Retained<NSApplication> =
        unsafe { msg_send![ShellApplication::class(), sharedApplication] };
    if !app.isKindOfClass(ShellApplication::class()) {
        return Err("NSApplication was created before the shell");
    }
    // A shell started outside the app bundle (during development) still gets a Dock icon.
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    Ok(())
}

fn item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<Sel>,
    key: &str,
) -> Retained<NSMenuItem> {
    // SAFETY: every action is a standard responder selector that AppKit routes itself.
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(key),
        )
    }
}

fn submenu(
    mtm: MainThreadMarker,
    bar: &NSMenu,
    title: &str,
    items: &[Retained<NSMenuItem>],
) -> Retained<NSMenu> {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
    for entry in items {
        menu.addItem(entry);
    }
    let holder = item(mtm, title, None, "");
    holder.setSubmenu(Some(&menu));
    bar.addItem(&holder);
    menu
}

/// The main menu: quit, the edit commands and the window menu. Browser shortcuts such as
/// ⌘T or ⌘W stay with the shell's key handler and are deliberately not menu items.
pub fn install_menu() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let bar = NSMenu::new(mtm);
    let separator = || NSMenuItem::separatorItem(mtm);
    submenu(
        mtm,
        &bar,
        "Zuruf",
        &[
            item(mtm, "Zuruf ausblenden", Some(sel!(hide:)), "h"),
            separator(),
            item(mtm, "Zuruf beenden", Some(sel!(terminate:)), "q"),
        ],
    );
    submenu(
        mtm,
        &bar,
        "Bearbeiten",
        &[
            item(mtm, "Widerrufen", Some(sel!(undo:)), "z"),
            // An upper-case key equivalent means Shift+⌘.
            item(mtm, "Wiederholen", Some(sel!(redo:)), "Z"),
            separator(),
            item(mtm, "Ausschneiden", Some(sel!(cut:)), "x"),
            item(mtm, "Kopieren", Some(sel!(copy:)), "c"),
            item(mtm, "Einsetzen", Some(sel!(paste:)), "v"),
            item(mtm, "Alles auswählen", Some(sel!(selectAll:)), "a"),
        ],
    );
    let window = submenu(
        mtm,
        &bar,
        "Fenster",
        &[item(
            mtm,
            "Im Dock ablegen",
            Some(sel!(performMiniaturize:)),
            "m",
        )],
    );
    app.setMainMenu(Some(&bar));
    app.setWindowsMenu(Some(&window));
}

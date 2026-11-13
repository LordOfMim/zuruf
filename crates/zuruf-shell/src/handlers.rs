//! Chromium's callbacks, forwarded to the window logic in `shell`.
use crate::{commands, shell};
use cef::*;
use std::cell::RefCell;

/// The platform's own key event, which CEF hands through untouched. The bindings pass a
/// reference on Linux and Windows and the raw NSEvent pointer on macOS.
#[cfg(target_os = "linux")]
type NativeEvent<'a> = Option<&'a mut sys::XEvent>;
#[cfg(target_os = "windows")]
type NativeEvent<'a> = Option<&'a mut sys::MSG>;
#[cfg(target_os = "macos")]
type NativeEvent<'a> = *mut u8;

/// A string for a structure that CEF reads after our callback has returned. The bindings pass
/// such a field on only if it wraps a raw CEF string, which CEF then frees itself; a string
/// built with `CefString::from(&str)` would arrive empty.
fn handed_over(text: &str) -> CefString {
    let mut raw: sys::_cef_string_utf16_t = unsafe { std::mem::zeroed() };
    // SAFETY: `text` is valid UTF-8 of the given length and `raw` is an empty CEF string for
    // the function to fill. It allocates the copy and records how to free it.
    unsafe {
        sys::cef_string_utf8_to_utf16(text.as_ptr().cast(), text.len(), &mut raw);
    }
    CefString::from(raw)
}

pub fn client() -> Client {
    ShellClient::new()
}
pub fn window() -> WindowDelegate {
    ShellWindow::new()
}
pub fn page_view() -> BrowserViewDelegate {
    PageView::new(0)
}
pub fn panel_view(width: i32) -> BrowserViewDelegate {
    PageView::new(width)
}
pub fn line() -> PanelDelegate {
    Line::new()
}
pub fn page_area() -> PanelDelegate {
    PageArea::new()
}
pub fn button() -> ButtonDelegate {
    Pressed::new()
}
pub fn address() -> TextfieldDelegate {
    Address::new()
}

wrap_client! {
    struct ShellClient;

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(Display::new())
        }
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(Keys::new())
        }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(Life::new())
        }
        fn load_handler(&self) -> Option<LoadHandler> {
            Some(Loading::new())
        }
        fn request_handler(&self) -> Option<RequestHandler> {
            Some(Recovery::new())
        }
    }
}

wrap_request_handler! {
    struct Recovery;

    impl RequestHandler {
        fn on_render_process_terminated(
            &self,
            _browser: Option<&mut Browser>,
            _status: TerminationStatus,
            _error_code: ::std::os::raw::c_int,
            _error_string: Option<&CefString>,
        ) {
            commands::emit("crashed");
        }

        fn on_render_process_unresponsive(
            &self,
            _browser: Option<&mut Browser>,
            callback: Option<&mut UnresponsiveProcessCallback>,
        ) -> ::std::os::raw::c_int {
            commands::emit("crashed");
            if let Some(callback) = callback {
                callback.terminate();
            }
            // The daemon restores the session; suppress Chromium's hung-page dialog.
            1
        }
    }
}

wrap_display_handler! {
    struct Display;

    impl DisplayHandler {
        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            if let Some(browser) = browser {
                let title = title.map(CefString::to_string).unwrap_or_default();
                shell::title_changed(browser, &title);
            }
        }

        fn on_address_change(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            let (Some(browser), Some(frame)) = (browser, frame) else {
                return;
            };
            if frame.is_main() != 0 {
                let url = url.map(CefString::to_string).unwrap_or_default();
                shell::address_changed(browser, &url);
            }
        }
    }
}

wrap_life_span_handler! {
    struct Life;

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                shell::browser_created(browser);
            }
        }

        fn do_close(&self, browser: Option<&mut Browser>) -> ::std::os::raw::c_int {
            match browser {
                Some(browser) => i32::from(shell::browser_closing(browser)),
                None => 0,
            }
        }
    }
}

wrap_load_handler! {
    struct Loading;

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            _can_go_back: ::std::os::raw::c_int,
            _can_go_forward: ::std::os::raw::c_int,
        ) {
            if let Some(browser) = browser {
                shell::loading_changed(browser, is_loading != 0);
            }
        }
    }
}

wrap_keyboard_handler! {
    struct Keys;

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: NativeEvent<'_>,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let (Some(browser), Some(event)) = (browser, event) else {
                return 0;
            };
            if event.type_ != KeyEventType::RAWKEYDOWN {
                return 0;
            }
            i32::from(shell::shortcut(browser, event.windows_key_code, event.modifiers))
        }
    }
}

// A page tab, or the app panel when it has a fixed width.
wrap_browser_view_delegate! {
    struct PageView {
        width: i32,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            // A size with an empty side counts as "no preference"; the layout stretches the
            // height anyway.
            Size {
                width: self.width,
                height: 100,
            }
        }
    }

    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }

        fn delegate_for_popup_browser_view(
            &self,
            _browser_view: Option<&mut BrowserView>,
            _settings: Option<&BrowserSettings>,
            _client: Option<&mut Client>,
            _is_devtools: ::std::os::raw::c_int,
        ) -> Option<BrowserViewDelegate> {
            Some(PageView::new(0))
        }

        fn on_popup_browser_view_created(
            &self,
            _browser_view: Option<&mut BrowserView>,
            popup_browser_view: Option<&mut BrowserView>,
            is_devtools: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let Some(popup) = popup_browser_view.cloned() else {
                return 0;
            };
            if is_devtools != 0 {
                // Developer tools get a window of their own.
                let mut delegate = ToolsWindow::new(RefCell::new(Some(popup)));
                window_create_top_level(Some(&mut delegate));
            } else {
                // Pages that open a window get a tab instead; they keep their opener.
                shell::adopt_tab(popup, "");
            }
            1
        }
    }
}

// Unlike CEF's C++ defaults, an unanswered question here means "no": a window that does not
// say it can be resized is fixed-size, and tiling compositors then float it.
wrap_window_delegate! {
    struct ShellWindow {}

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size {
                width: 1500,
                height: 950,
            }
        }

        fn minimum_size(&self, _view: Option<&mut View>) -> Size {
            Size {
                width: 640,
                height: 400,
            }
        }

        fn on_theme_changed(&self, _view: Option<&mut View>) {
            shell::theme_changed();
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn can_resize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn can_maximize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn can_minimize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn on_window_created(&self, window: Option<&mut Window>) {
            if let Some(window) = window {
                shell::build(window);
            }
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            shell::destroyed();
            quit_message_loop();
        }

        fn on_window_bounds_changed(&self, _window: Option<&mut Window>, _bounds: Option<&Rect>) {
            shell::fit_page();
        }

        fn initial_bounds(&self, _window: Option<&mut Window>) -> Rect {
            Rect {
                x: 0,
                y: 0,
                width: 1500,
                height: 950,
            }
        }

        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            shell::window_closing();
            1
        }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }

        fn on_theme_colors_changed(
            &self,
            window: Option<&mut Window>,
            _chrome_theme: ::std::os::raw::c_int,
        ) {
            if let Some(window) = window {
                shell::theme(window);
            }
        }

        fn linux_window_properties(
            &self,
            _window: Option<&mut Window>,
            properties: Option<&mut LinuxWindowProperties>,
        ) -> ::std::os::raw::c_int {
            let Some(properties) = properties else {
                return 0;
            };
            // Window rules and the desktop entry find the window by this name.
            properties.wayland_app_id = handed_over("zuruf");
            properties.wm_class_class = handed_over("zuruf");
            properties.wm_class_name = handed_over("zuruf");
            1
        }
    }
}

wrap_window_delegate! {
    struct ToolsWindow {
        view: RefCell<Option<BrowserView>>,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size {
                width: 1100,
                height: 760,
            }
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn can_resize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn can_maximize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn can_minimize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn on_window_created(&self, window: Option<&mut Window>) {
            let view = self.view.borrow();
            let (Some(window), Some(view)) = (window, view.as_ref()) else {
                return;
            };
            window.add_child_view(Some(&mut View::from(view)));
            window.set_title(Some(&CefString::from("Zuruf – Entwicklerwerkzeuge")));
            window.show();
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            *self.view.borrow_mut() = None;
        }

        fn initial_bounds(&self, _window: Option<&mut Window>) -> Rect {
            Rect {
                x: 0,
                y: 0,
                width: 1100,
                height: 760,
            }
        }

        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

wrap_panel_delegate! {
    struct Line {}

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size {
                width: 1,
                height: 1,
            }
        }
    }

    impl PanelDelegate {}
}

wrap_panel_delegate! {
    struct PageArea {}

    impl ViewDelegate {
        fn on_layout_changed(&self, _view: Option<&mut View>, _new_bounds: Option<&Rect>) {
            // Its width is known now; centre the page if a fixed width is chosen.
            shell::fit_page();
        }
    }

    impl PanelDelegate {}
}

wrap_button_delegate! {
    struct Pressed {}

    impl ViewDelegate {}

    impl ButtonDelegate {
        fn on_button_pressed(&self, button: Option<&mut Button>) {
            if let Some(button) = button {
                shell::press_later(button.id());
            }
        }
    }
}

wrap_textfield_delegate! {
    struct Address {}

    impl ViewDelegate {}

    impl TextfieldDelegate {
        fn on_key_event(
            &self,
            textfield: Option<&mut Textfield>,
            event: Option<&KeyEvent>,
        ) -> ::std::os::raw::c_int {
            const ENTER: i32 = 0x0D;
            let (Some(textfield), Some(event)) = (textfield, event) else {
                return 0;
            };
            if event.type_ != KeyEventType::RAWKEYDOWN || event.windows_key_code != ENTER {
                return 0;
            }
            let typed = CefString::from(&textfield.text()).to_string();
            shell::navigate(&typed);
            1
        }
    }
}

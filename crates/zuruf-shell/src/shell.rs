//! The window: the app panel on the left; tab strip, toolbar and the visible page on the right.
//!
//! Everything here runs on Chromium's UI thread. The state lives in a thread-local cell and is
//! only borrowed for short reads and writes, never across a call into CEF: such calls may come
//! straight back into one of our handlers.
use crate::{
    commands, handlers, options,
    options::{Options, Theme},
};
use cef::*;
use std::cell::RefCell;

/// Colours of one of the app's themes (`apps/ui/src/styles/tokens.css`), as ARGB.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Behind everything: the tab strip and the empty page area.
    pub background: u32,
    /// The toolbar and the visible tab.
    surface: u32,
    /// The address field.
    field: u32,
    text: u32,
    muted: u32,
    /// The one accent: focus and selection.
    signal: u32,
}
const DARK: Palette = Palette {
    background: 0xFF0E_0F10,
    surface: 0xFF15_1618,
    field: 0xFF1C_1D20,
    text: 0xFFE8_E7E3,
    muted: 0xFF9B_9C98,
    signal: 0xFFFF_5A36,
};
const LIGHT: Palette = Palette {
    background: 0xFFF6_F5F2,
    surface: 0xFFFF_FFFF,
    field: 0xFFF1_F0EC,
    text: 0xFF1A_1A18,
    muted: 0xFF5F_605C,
    signal: 0xFFE8_431F,
};

/// The palette to start with, before the desktop's preference is known.
pub fn initial_palette(theme: Theme) -> Palette {
    if theme == Theme::Light { LIGHT } else { DARK }
}

/// View ids of the toolbar buttons. Tab buttons use `TAB_BASE + tab id`.
pub const BACK: i32 = 1;
pub const FORWARD: i32 = 2;
pub const RELOAD: i32 = 3;
pub const NEW_TAB: i32 = 4;
pub const CLOSE_TAB: i32 = 5;
const TAB_BASE: i32 = 1000;

const BAR_HEIGHT: i32 = 34;
const TAB_WIDTH: i32 = 220;
const BLANK: &str = "about:blank";

struct Tab {
    id: i32,
    /// Chromium's identifier of the tab's browser, once it is known.
    browser: Option<i32>,
    view: BrowserView,
    button: LabelButton,
    title: String,
    url: String,
}

#[derive(Default)]
struct Shell {
    options: Options,
    window: Option<Window>,
    panel: Option<BrowserView>,
    strip: Option<Panel>,
    toolbar: Option<Panel>,
    stack: Option<Panel>,
    address: Option<Textfield>,
    tabs: Vec<Tab>,
    active: Option<i32>,
    next_tab: i32,
    /// The window is going away; browsers may close without further questions.
    closing: bool,
    theme: Theme,
    /// What the desktop prefers, as far as Chromium has told us.
    desktop_dark: Option<bool>,
}

thread_local! {
    static SHELL: RefCell<Shell> = RefCell::default();
}

fn with<R>(action: impl FnOnce(&mut Shell) -> R) -> R {
    SHELL.with(|shell| action(&mut shell.borrow_mut()))
}

fn text(value: &str) -> CefString {
    CefString::from(value)
}

fn palette() -> Palette {
    with(|shell| {
        let dark = match shell.theme {
            Theme::Dark => true,
            Theme::Light => false,
            Theme::System => shell.desktop_dark.unwrap_or(true),
        };
        if dark { DARK } else { LIGHT }
    })
}

/// The panel starts on the app's own surface, so it never flashes in the wrong colour.
fn panel_settings() -> BrowserSettings {
    BrowserSettings {
        background_color: palette().background,
        ..Default::default()
    }
}

/// Pages keep the web's default: white until they say otherwise.
fn page_settings() -> BrowserSettings {
    BrowserSettings {
        background_color: 0xFFFF_FFFF,
        ..Default::default()
    }
}

/// Dress Chromium's own widgets (buttons, address field) in the app's colours. Pages are not
/// affected: their colour scheme keeps following the desktop.
pub fn theme(window: &Window) {
    use sys::cef_color_id_t as color;
    // While Chromium's own colours are in place they tell us what the desktop prefers.
    // Our own colours say nothing, so those are skipped.
    let current = window.theme_color(color::CEF_ColorPrimaryBackground as i32);
    if current != DARK.background && current != LIGHT.background {
        let brightness = ((current >> 16) & 0xFF) + ((current >> 8) & 0xFF) + (current & 0xFF);
        with(|shell| shell.desktop_dark = Some(brightness < 3 * 128));
    }
    let palette = palette();
    for (id, value) in [
        (color::CEF_ColorPrimaryBackground, palette.background),
        (color::CEF_ColorWindowBackground, palette.background),
        (color::CEF_ColorPrimaryForeground, palette.text),
        (color::CEF_ColorLabelForeground, palette.text),
        (color::CEF_ColorButtonForeground, palette.text),
        (color::CEF_ColorButtonBackground, palette.background),
        (color::CEF_ColorTextfieldBackground, palette.field),
        (color::CEF_ColorTextfieldForeground, palette.text),
        (color::CEF_ColorTextfieldForegroundPlaceholder, palette.muted),
        (color::CEF_ColorTextfieldOutline, palette.surface),
        (color::CEF_ColorTextfieldSelectionBackground, palette.signal),
        (color::CEF_ColorTextfieldSelectionForeground, palette.background),
        (color::CEF_ColorFocusableBorderFocused, palette.signal),
    ] {
        window.set_theme_color(id as i32, value);
    }
    paint();
}

/// Colour the views that carry a colour of their own.
fn paint() {
    let palette = palette();
    let (strip, toolbar, stack, active, tabs) = with(|shell| {
        let tabs: Vec<(i32, LabelButton)> = shell
            .tabs
            .iter()
            .map(|tab| (tab.id, tab.button.clone()))
            .collect();
        (
            shell.strip.clone(),
            shell.toolbar.clone(),
            shell.stack.clone(),
            shell.active,
            tabs,
        )
    });
    if let Some(strip) = strip {
        strip.set_background_color(palette.background);
    }
    if let Some(toolbar) = toolbar {
        toolbar.set_background_color(palette.surface);
    }
    if let Some(stack) = stack {
        stack.set_background_color(palette.background);
    }
    for (id, button) in tabs {
        let shown = active == Some(id);
        button.set_enabled_text_colors(if shown { palette.text } else { palette.muted });
        button.set_background_color(if shown {
            palette.surface
        } else {
            palette.background
        });
    }
}

/// Wear another theme, now.
pub fn set_theme(theme_mode: Theme) {
    let window = with(|shell| {
        shell.theme = theme_mode;
        shell.window.clone()
    });
    if let Some(window) = window {
        theme(&window);
        window.theme_changed();
    }
}

/// Create the window. Called once Chromium is up.
pub fn create(options: Options) {
    with(|shell| shell.theme = options.theme);
    let panel = options.panel.as_deref().and_then(|url| {
        let mut client = handlers::client();
        let mut delegate = handlers::panel_view(options.panel_width);
        browser_view_create(
            Some(&mut client),
            Some(&text(url)),
            Some(&panel_settings()),
            None,
            None,
            Some(&mut delegate),
        )
    });
    with(|shell| {
        shell.options = options;
        shell.panel = panel;
    });
    let mut delegate = handlers::window();
    window_create_top_level(Some(&mut delegate));
}

fn row(spacing: i32) -> BoxLayoutSettings {
    BoxLayoutSettings {
        horizontal: 1,
        between_child_spacing: spacing,
        cross_axis_alignment: AxisAlignment::STRETCH,
        ..Default::default()
    }
}

fn column() -> BoxLayoutSettings {
    BoxLayoutSettings {
        horizontal: 0,
        cross_axis_alignment: AxisAlignment::STRETCH,
        ..Default::default()
    }
}

fn tool_button(id: i32, glyph: &str, tooltip: &str) -> Option<LabelButton> {
    let mut delegate = handlers::button();
    let button = label_button_create(Some(&mut delegate), Some(&text(glyph)))?;
    button.set_id(id);
    button.set_tooltip_text(Some(&text(tooltip)));
    button.set_accessible_name(Some(&text(tooltip)));
    button.set_minimum_size(Some(&Size {
        width: BAR_HEIGHT,
        height: BAR_HEIGHT,
    }));
    Some(button)
}

/// Fill the freshly created window with its views and open the first tabs.
pub fn build(window: &Window) {
    window.set_title(Some(&text("Zuruf")));
    theme(window);
    window.theme_changed();
    let layout = window.set_to_box_layout(Some(&row(0)));
    let (panel, urls, app_only) = with(|shell| {
        (
            shell.panel.clone(),
            shell.options.urls.clone(),
            shell.options.app_only,
        )
    });
    if let Some(panel) = &panel {
        let mut view = View::from(panel);
        window.add_child_view(Some(&mut view));
        if app_only {
            // The app alone: it takes the whole window.
            if let Some(layout) = &layout {
                layout.set_flex_for_view(Some(&mut view), 1);
            }
            with(|shell| shell.window = Some(window.clone()));
            window.show();
            view.request_focus();
            commands::emit("ready");
            return;
        }
    }

    let (Some(right), Some(strip), Some(toolbar), Some(stack)) = (
        panel_create(None),
        panel_create(None),
        panel_create(None),
        panel_create(None),
    ) else {
        return;
    };
    let right_layout = right.set_to_box_layout(Some(&column()));

    strip.set_to_box_layout(Some(&row(1)));
    if let Some(add) = tool_button(NEW_TAB, "+", "Neuer Tab (Strg+T)") {
        strip.add_child_view(Some(&mut View::from(&add)));
    }

    let toolbar_layout = toolbar.set_to_box_layout(Some(&row(0)));
    for (id, glyph, tooltip) in [
        (BACK, "←", "Zurück (Alt+←)"),
        (FORWARD, "→", "Vorwärts (Alt+→)"),
        (RELOAD, "↻", "Neu laden (Strg+R)"),
    ] {
        if let Some(button) = tool_button(id, glyph, tooltip) {
            toolbar.add_child_view(Some(&mut View::from(&button)));
        }
    }
    let mut address_delegate = handlers::address();
    let address = textfield_create(Some(&mut address_delegate));
    if let Some(address) = &address {
        address.set_placeholder_text(Some(&text("Adresse eingeben")));
        address.set_accessible_name(Some(&text("Adresse")));
        let mut view = View::from(address);
        toolbar.add_child_view(Some(&mut view));
        if let Some(layout) = &toolbar_layout {
            layout.set_flex_for_view(Some(&mut view), 1);
        }
    }
    if let Some(close) = tool_button(CLOSE_TAB, "×", "Tab schließen (Strg+W)") {
        toolbar.add_child_view(Some(&mut View::from(&close)));
    }

    stack.set_to_fill_layout();

    right.add_child_view(Some(&mut View::from(&strip)));
    right.add_child_view(Some(&mut View::from(&toolbar)));
    let mut stack_view = View::from(&stack);
    right.add_child_view(Some(&mut stack_view));
    if let Some(layout) = &right_layout {
        layout.set_flex_for_view(Some(&mut stack_view), 1);
    }
    let mut right_view = View::from(&right);
    window.add_child_view(Some(&mut right_view));
    if let Some(layout) = &layout {
        layout.set_flex_for_view(Some(&mut right_view), 1);
    }

    with(|shell| {
        shell.window = Some(window.clone());
        shell.strip = Some(strip);
        shell.toolbar = Some(toolbar);
        shell.stack = Some(stack);
        shell.address = address;
    });
    paint();
    for url in &urls {
        open_tab(url);
    }
    if let Some(first) = with(|shell| shell.tabs.first().map(|tab| tab.id)) {
        activate(first);
    }
    window.show();
    commands::emit("ready");
}

/// Forget every view once the window is gone.
pub fn destroyed() {
    with(|shell| *shell = Shell::default());
}

/// Open a page in a new tab and show it.
pub fn open_tab(url: &str) {
    let mut client = handlers::client();
    let mut delegate = handlers::page_view();
    let view = browser_view_create(
        Some(&mut client),
        Some(&text(url)),
        Some(&page_settings()),
        None,
        None,
        Some(&mut delegate),
    );
    if let Some(view) = view {
        adopt_tab(view, url);
    }
}

/// Give a page view (ours, or one a page opened itself) a tab and show it.
pub fn adopt_tab(view: BrowserView, url: &str) {
    let mut delegate = handlers::button();
    let Some(button) = label_button_create(Some(&mut delegate), Some(&text(&tab_label("", url))))
    else {
        return;
    };
    button.set_maximum_size(Some(&Size {
        width: TAB_WIDTH,
        height: BAR_HEIGHT,
    }));
    button.set_minimum_size(Some(&Size {
        width: 80,
        height: BAR_HEIGHT,
    }));
    let placed = with(|shell| {
        let (strip, stack) = (shell.strip.clone()?, shell.stack.clone()?);
        shell.next_tab += 1;
        let id = shell.next_tab;
        let position = shell.tabs.len() as i32;
        shell.tabs.push(Tab {
            id,
            browser: None,
            view: view.clone(),
            button: button.clone(),
            title: String::new(),
            url: url.into(),
        });
        Some((id, position, strip, stack))
    });
    let Some((id, position, strip, stack)) = placed else {
        return;
    };
    button.set_id(TAB_BASE + id);
    // Tabs sit in front of the "new tab" button.
    strip.add_child_view_at(Some(&mut View::from(&button)), position);
    stack.add_child_view(Some(&mut View::from(&view)));
    activate(id);
}

fn tab_label(title: &str, url: &str) -> String {
    let label = if !title.is_empty() && title != BLANK {
        title
    } else if url.is_empty() || url == BLANK {
        "Neuer Tab"
    } else {
        url.split("://").nth(1).unwrap_or(url)
    };
    let mut short: String = label.chars().take(28).collect();
    if label.chars().count() > 28 {
        short.push('…');
    }
    short
}

/// Show one tab and hide the others.
pub fn activate(id: i32) {
    let found = with(|shell| {
        let tab = shell.tabs.iter().find(|tab| tab.id == id)?;
        let current = (tab.url.clone(), tab.title.clone(), tab.view.clone());
        shell.active = Some(id);
        let all: Vec<(i32, BrowserView)> = shell
            .tabs
            .iter()
            .map(|tab| (tab.id, tab.view.clone()))
            .collect();
        Some((
            current,
            all,
            shell.address.clone(),
            shell.stack.clone(),
            shell.window.clone(),
        ))
    });
    let Some(((url, title, view), all, address, stack, window)) = found else {
        return;
    };
    for (tab, view) in all {
        View::from(&view).set_visible(i32::from(tab == id));
    }
    paint();
    if let Some(address) = address {
        address.set_text(Some(&text(shown_address(&url))));
    }
    if let Some(stack) = stack {
        stack.layout();
    }
    if let Some(window) = window {
        window.set_title(Some(&text(&window_title(&title))));
    }
    View::from(&view).request_focus();
}

fn shown_address(url: &str) -> &str {
    if url == BLANK { "" } else { url }
}

fn window_title(title: &str) -> String {
    if title.is_empty() || title == BLANK {
        "Zuruf".into()
    } else {
        format!("{title} – Zuruf")
    }
}

/// Close a tab. The window always keeps one, so the last tab is replaced by an empty one.
pub fn close_tab(id: i32) {
    let removed = with(|shell| {
        let index = shell.tabs.iter().position(|tab| tab.id == id)?;
        let tab = shell.tabs.remove(index);
        let neighbour = shell
            .tabs
            .get(index)
            .or(shell.tabs.last())
            .map(|tab| tab.id);
        let was_active = shell.active == Some(id);
        Some((
            tab,
            neighbour,
            was_active,
            shell.strip.clone(),
            shell.stack.clone(),
        ))
    });
    let Some((tab, neighbour, was_active, strip, stack)) = removed else {
        return;
    };
    if let Some(strip) = strip {
        strip.remove_child_view(Some(&mut View::from(&tab.button)));
        strip.layout();
    }
    // Taking the view out of the window and letting go of it destroys its browser.
    if let Some(stack) = stack {
        stack.remove_child_view(Some(&mut View::from(&tab.view)));
    }
    drop(tab);
    match neighbour {
        Some(next) if was_active => activate(next),
        Some(_) => {}
        None => open_tab(BLANK),
    }
}

fn active_tab() -> Option<(i32, BrowserView)> {
    with(|shell| {
        let id = shell.active?;
        let tab = shell.tabs.iter().find(|tab| tab.id == id)?;
        Some((id, tab.view.clone()))
    })
}

fn active_browser() -> Option<Browser> {
    active_tab()?.1.browser()
}

/// The tab a browser belongs to; `None` for the panel and for developer tools.
fn tab_of(browser: &mut Browser) -> Option<i32> {
    let identifier = browser.identifier();
    let known = with(|shell| {
        shell
            .tabs
            .iter()
            .find(|tab| tab.browser == Some(identifier))
            .map(|tab| tab.id)
    });
    if known.is_some() {
        return known;
    }
    // First event of this browser: find its view and remember the identifier, which stays
    // valid while the browser is being closed and its view is already gone.
    let view = browser_view_get_for_browser(Some(browser))?;
    let mut view = View::from(&view);
    with(|shell| {
        let tab = shell
            .tabs
            .iter_mut()
            .find(|tab| View::from(&tab.view).is_same(Some(&mut view)) != 0)?;
        tab.browser = Some(identifier);
        Some(tab.id)
    })
}

/// A browser exists now; learn which tab it is.
pub fn browser_created(browser: &mut Browser) {
    tab_of(browser);
}

wrap_task! {
    struct CloseTab {
        id: i32,
    }

    impl Task {
        fn execute(&self) {
            close_tab(self.id);
        }
    }
}

/// A browser wants to close: an agent closed its target, or the page called `window.close()`.
/// Returns true when we take care of it by removing the tab. Otherwise Chromium would close
/// the whole window, which is only right while the window itself is closing.
pub fn browser_closing(browser: &mut Browser) -> bool {
    if with(|shell| shell.closing) {
        return false;
    }
    let Some(id) = tab_of(browser) else {
        return false;
    };
    // Not from inside the callback: removing the view destroys the browser we were called for.
    let mut task = CloseTab::new(id);
    post_task(ThreadId::UI, Some(&mut task));
    true
}

/// The window is about to close.
pub fn window_closing() {
    with(|shell| shell.closing = true);
}

pub fn title_changed(browser: &mut Browser, title: &str) {
    let Some(id) = tab_of(browser) else {
        return;
    };
    let update = with(|shell| {
        let active = shell.active == Some(id);
        let window = shell.window.clone();
        let tab = shell.tabs.iter_mut().find(|tab| tab.id == id)?;
        tab.title = title.into();
        Some((
            tab.button.clone(),
            tab_label(&tab.title, &tab.url),
            active,
            window,
        ))
    });
    let Some((button, label, active, window)) = update else {
        return;
    };
    button.set_text(Some(&text(&label)));
    button.set_tooltip_text(Some(&text(title)));
    if active && let Some(window) = window {
        window.set_title(Some(&text(&window_title(title))));
    }
}

pub fn address_changed(browser: &mut Browser, url: &str) {
    let Some(id) = tab_of(browser) else {
        return;
    };
    let update = with(|shell| {
        let active = shell.active == Some(id);
        let address = shell.address.clone();
        let tab = shell.tabs.iter_mut().find(|tab| tab.id == id)?;
        tab.url = url.into();
        Some((
            tab.button.clone(),
            tab_label(&tab.title, &tab.url),
            active,
            address,
        ))
    });
    let Some((button, label, active, address)) = update else {
        return;
    };
    button.set_text(Some(&text(&label)));
    if active && let Some(address) = address {
        address.set_text(Some(&text(shown_address(url))));
    }
}

/// Load what was typed into the address field in the visible tab.
pub fn navigate(input: &str) {
    let Some((_, view)) = active_tab() else {
        return;
    };
    if let Some(frame) = view.browser().and_then(|browser| browser.main_frame()) {
        frame.load_url(Some(&text(&options::address(input))));
    }
    View::from(&view).request_focus();
}

/// A toolbar or tab button was pressed.
pub fn pressed(id: i32) {
    match id {
        BACK => {
            if let Some(browser) = active_browser() {
                browser.go_back();
            }
        }
        FORWARD => {
            if let Some(browser) = active_browser() {
                browser.go_forward();
            }
        }
        RELOAD => {
            if let Some(browser) = active_browser() {
                browser.reload();
            }
        }
        NEW_TAB => new_tab(),
        CLOSE_TAB => {
            if let Some((id, _)) = active_tab() {
                close_tab(id);
            }
        }
        tab if tab > TAB_BASE => activate(tab - TAB_BASE),
        _ => {}
    }
}

fn new_tab() {
    open_tab(BLANK);
    focus_address();
}

fn focus_address() {
    if let Some(address) = with(|shell| shell.address.clone()) {
        address.request_focus();
        address.select_all(0);
    }
}

fn step_tab(forward: bool) {
    let next = with(|shell| {
        let count = shell.tabs.len();
        let index = shell
            .tabs
            .iter()
            .position(|tab| Some(tab.id) == shell.active)?;
        let step = if forward { 1 } else { count - 1 };
        Some(shell.tabs[(index + step) % count].id)
    });
    if let Some(id) = next {
        activate(id);
    }
}

fn open_dev_tools(browser: &Browser) {
    if let Some(host) = browser.host() {
        let mut client = handlers::client();
        host.show_dev_tools(
            Some(&WindowInfo::default()),
            Some(&mut client),
            Some(&BrowserSettings::default()),
            None,
        );
    }
}

/// Windows virtual key codes, which Chromium reports on every platform.
mod key {
    pub const TAB: i32 = 0x09;
    pub const LEFT: i32 = 0x25;
    pub const RIGHT: i32 = 0x27;
    pub const ZERO: i32 = 0x30;
    pub const L: i32 = 0x4C;
    pub const R: i32 = 0x52;
    pub const T: i32 = 0x54;
    pub const W: i32 = 0x57;
    pub const F5: i32 = 0x74;
    pub const F12: i32 = 0x7B;
    pub const PLUS: i32 = 0xBB;
    pub const MINUS: i32 = 0xBD;
}
const SHIFT: u32 = 1 << 1;
const CONTROL: u32 = 1 << 2;
const ALT: u32 = 1 << 3;

/// Browser shortcuts. Returns true when the key was ours, so the page never sees it.
/// Review hotkeys are not handled here: the page agent decides those inside the page.
pub fn shortcut(browser: &mut Browser, code: i32, modifiers: u32) -> bool {
    let held = modifiers & (SHIFT | CONTROL | ALT);
    let in_tab = tab_of(browser).is_some();
    let zoom = |command: ZoomCommand| {
        if let Some(host) = browser.host() {
            host.zoom(command);
        }
    };
    match (held, code) {
        (CONTROL, key::T) => new_tab(),
        (CONTROL, key::W) => {
            if let Some((id, _)) = active_tab() {
                close_tab(id);
            }
        }
        (CONTROL, key::L) => focus_address(),
        (CONTROL, key::TAB) => step_tab(true),
        (held, key::TAB) if held == CONTROL | SHIFT => step_tab(false),
        (CONTROL, key::R) | (0, key::F5) if in_tab => browser.reload(),
        (held, key::R) if held == CONTROL | SHIFT && in_tab => browser.reload_ignore_cache(),
        (ALT, key::LEFT) if in_tab => browser.go_back(),
        (ALT, key::RIGHT) if in_tab => browser.go_forward(),
        (CONTROL, key::PLUS) if in_tab => zoom(ZoomCommand::IN),
        (CONTROL, key::MINUS) if in_tab => zoom(ZoomCommand::OUT),
        (CONTROL, key::ZERO) if in_tab => zoom(ZoomCommand::RESET),
        (0, key::F12) => open_dev_tools(browser),
        _ => return false,
    }
    true
}

/// Bring the window to the front.
pub fn focus_window() {
    if let Some(window) = with(|shell| shell.window.clone()) {
        window.show();
        window.activate();
    }
}

/// Show or hide the app panel; toggles when `visible` is `None`.
pub fn show_panel(visible: Option<bool>) {
    let (Some(panel), Some(window)) = with(|shell| (shell.panel.clone(), shell.window.clone()))
    else {
        return;
    };
    let view = View::from(&panel);
    let show = visible.unwrap_or(view.is_visible() == 0);
    view.set_visible(i32::from(show));
    window.layout();
}

/// Close the window; the process ends once it is gone.
pub fn quit() {
    match with(|shell| shell.window.clone()) {
        Some(window) => window.close(),
        None => quit_message_loop(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(tab_label("", "about:blank"), "Neuer Tab");
        assert_eq!(tab_label("", "http://localhost:5173/"), "localhost:5173/");
        assert_eq!(tab_label("Start", "http://localhost/"), "Start");
        assert_eq!(tab_label(&"x".repeat(40), "").chars().count(), 29);
        assert_eq!(window_title("Start"), "Start – Zuruf");
    }
}

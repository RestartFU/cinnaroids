#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod engine;
mod settings;
mod window_frame;

use engine::{Engine, ToggleKey};
use gpui::{prelude::*, *};
use settings::Settings;
use std::{borrow::Cow, cell::Cell, rc::Rc, time::Duration};

struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "mark.svg" => Some(Cow::Borrowed(include_bytes!("../assets/mark.svg"))),
            "minimize.svg" => Some(Cow::Borrowed(include_bytes!("../assets/minimize.svg"))),
            "maximize.svg" => Some(Cow::Borrowed(include_bytes!("../assets/maximize.svg"))),
            "restore.svg" => Some(Cow::Borrowed(include_bytes!("../assets/restore.svg"))),
            "close.svg" => Some(Cow::Borrowed(include_bytes!("../assets/close.svg"))),
            _ => None,
        })
    }
    fn list(&self, _: &str) -> gpui::Result<Vec<SharedString>> {
        Ok([
            "mark.svg",
            "minimize.svg",
            "maximize.svg",
            "restore.svg",
            "close.svg",
        ]
        .into_iter()
        .map(Into::into)
        .collect())
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Clicker,
    Settings,
}
struct Client {
    engine: Engine,
    preferences: Settings,
    page: Page,
    error: Option<String>,
    startup_error: Option<String>,
    preview: bool,
    was_enabled: bool,
    binding_revision: u64,
    slider_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    dragging: bool,
}
impl Client {
    fn new(preview: bool, page: Page, window: &Window, cx: &mut Context<Self>) -> Self {
        let (mut preferences, error) = if preview {
            (Settings::default(), None)
        } else {
            Settings::load()
        };
        let mut startup_error = None;
        let engine = if preview {
            Engine::new_preview()
        } else {
            match Engine::new() {
                Ok(engine) => engine,
                Err(message) => {
                    startup_error = Some(message);
                    Engine::new_preview()
                }
            }
        };
        engine.set_cps(preferences.cps);
        let key = ToggleKey::from_label(&preferences.toggle_key).unwrap_or(ToggleKey::F8);
        preferences.toggle_key = key.storage_label();
        engine.set_toggle_key(key);
        let binding_revision = engine.snapshot().binding_revision;
        cx.spawn_in(window, async move |view, cx| {
            loop {
                Timer::after(Duration::from_millis(80)).await;
                if cx
                    .update(|window, cx| {
                        view.update(cx, |s, cx| {
                            let snapshot = s.engine.snapshot();
                            if snapshot.enabled
                                && !s.was_enabled
                                && s.preferences.minimize_on_enable
                            {
                                window.minimize_window();
                            }
                            s.was_enabled = snapshot.enabled;
                            if snapshot.binding_revision != s.binding_revision {
                                s.binding_revision = snapshot.binding_revision;
                                s.preferences.toggle_key = snapshot.toggle_key.storage_label();
                                s.save();
                            }
                            if snapshot.capturing_key && !window.is_window_active() {
                                s.engine.cancel_key_capture();
                            }
                            cx.notify();
                        })
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            engine,
            preferences,
            page,
            error,
            startup_error,
            preview,
            was_enabled: false,
            binding_revision,
            slider_bounds: Rc::new(Cell::new(None)),
            dragging: false,
        }
    }
    fn tone(&self, dark: u32, light: u32) -> u32 {
        if self.preferences.dark_mode {
            dark
        } else {
            light
        }
    }
    fn tint(&self, dark: u32, light: u32) -> Rgba {
        rgba(self.tone(dark, light))
    }
    fn text(&self) -> u32 {
        self.tone(0xeeeeee, 0x282828)
    }
    fn muted(&self) -> u32 {
        self.tone(0xa3a3a3, 0x707070)
    }
    fn faint(&self) -> u32 {
        self.tone(0x858585, 0x858585)
    }
    fn accent(&self) -> u32 {
        self.tone(0xec8273, 0xdb6555)
    }
    fn accent_fill(&self, alpha: u8) -> Rgba {
        rgba((self.accent() << 8) | u32::from(alpha))
    }
    fn save(&mut self) {
        if !self.preview {
            self.error = self.preferences.save().err();
        }
    }
    fn set_cps(&mut self, cps: u32, cx: &mut Context<Self>) {
        self.preferences.cps = cps.clamp(1, 30);
        self.engine.set_cps(self.preferences.cps);
        self.save();
        cx.notify();
    }
    fn toggle(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.startup_error.is_none() {
            self.engine.toggle();
            cx.notify();
        }
    }
    fn set_slider(&mut self, x: Pixels, cx: &mut Context<Self>) {
        if let Some(bounds) = self.slider_bounds.get() {
            let ratio = ((x - bounds.origin.x) / bounds.size.width).clamp(0.0, 1.0);
            self.set_cps((1.0 + ratio * 29.0).round() as u32, cx);
        }
    }
    fn icon(path: &'static str, color: u32, size: f32) -> Svg {
        svg().path(path).size(px(size)).text_color(rgb(color))
    }
    fn panel(&self) -> Div {
        div()
            .bg(self.tint(0x242424fa, 0xfffffff0))
            .rounded(px(12.0))
            .border_1()
            .border_color(self.tint(0xffffff13, 0x0000000d))
    }
    fn divider(&self) -> Div {
        div()
            .h(px(1.0))
            .w_full()
            .bg(self.tint(0xffffff10, 0x0000000c))
    }
    fn switch(&self, on: bool) -> Div {
        div()
            .w(px(40.0))
            .h(px(23.0))
            .rounded_full()
            .bg(if on {
                self.accent_fill(255)
            } else {
                self.tint(0xffffff26, 0x00000025)
            })
            .flex()
            .items_center()
            .px(px(3.0))
            .when(on, |d| d.justify_end())
            .child(div().size(px(17.0)).rounded_full().bg(rgb(0xffffff)))
    }
    fn keycap(&self, key: impl Into<SharedString>) -> Div {
        div()
            .min_w(px(33.0))
            .px(px(9.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .bg(self.tint(0xffffff0a, 0x00000004))
            .border_1()
            .border_color(self.tint(0xffffff16, 0x00000014))
            .text_center()
            .text_size(px(11.0))
            .text_color(rgb(self.muted()))
            .child(key.into())
    }
    fn row(&self, title: &'static str, description: &'static str, right: AnyElement) -> Div {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .py(px(18.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(self.muted()))
                            .child(description),
                    ),
            )
            .child(right)
    }
    fn tab(&self, title: &'static str, page: Page, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.page == page;
        div()
            .id(title)
            .px(px(14.0))
            .h(px(34.0))
            .rounded(px(7.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .text_color(rgb(if selected { self.text() } else { self.muted() }))
            .bg(if selected {
                self.tint(0xffffff0e, 0xffffffce)
            } else {
                rgba(0)
            })
            .hover(|d| d.bg(self.tint(0xffffff10, 0xffffffd9)))
            .on_click(cx.listener(move |s, _, _, cx| {
                s.engine.cancel_key_capture();
                s.dragging = false;
                s.slider_bounds.set(None);
                s.page = page;
                cx.notify();
            }))
            .child(title)
            .into_any_element()
    }
    fn clicker_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let snapshot = self.engine.snapshot();
        let cps = snapshot.cps;
        let bounds = self.slider_bounds.clone();
        self.panel()
            .px(px(20.0))
            .child(
                self.row(
                    "Auto clicker",
                    "Hold left mouse to click. Release to pause.",
                    div()
                        .id("enable-switch")
                        .cursor_pointer()
                        .on_click(cx.listener(Self::toggle))
                        .child(self.switch(snapshot.enabled))
                        .into_any_element(),
                ),
            )
            .child(self.divider())
            .child(
                div()
                    .py(px(20.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap(px(10.0))
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .text_color(rgb(self.muted()))
                                            .child("Click speed"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(30.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(format!("{cps}")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(rgb(self.muted()))
                                            .child("CPS"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .id("slower")
                                            .size(px(30.0))
                                            .rounded(px(6.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .bg(self.tint(0xffffff08, 0x00000004))
                                            .hover(|d| d.bg(self.tint(0xffffff14, 0x00000009)))
                                            .text_size(px(18.0))
                                            .on_click(cx.listener(|s, _, _, cx| {
                                                s.set_cps(s.preferences.cps.saturating_sub(1), cx)
                                            }))
                                            .child("−"),
                                    )
                                    .child(
                                        div()
                                            .id("faster")
                                            .size(px(30.0))
                                            .rounded(px(6.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .bg(self.tint(0xffffff08, 0x00000004))
                                            .hover(|d| d.bg(self.tint(0xffffff14, 0x00000009)))
                                            .text_size(px(18.0))
                                            .on_click(cx.listener(|s, _, _, cx| {
                                                s.set_cps(s.preferences.cps + 1, cx)
                                            }))
                                            .child("+"),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("speed-slider")
                            .relative()
                            .h(px(30.0))
                            .mt(px(14.0))
                            .mx(px(8.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|s, e: &MouseDownEvent, _, cx| {
                                    s.dragging = true;
                                    s.set_slider(e.position.x, cx);
                                }),
                            )
                            .child(
                                canvas(
                                    move |rect, _, _| {
                                        bounds.set(Some(rect));
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(13.0))
                                    .w_full()
                                    .h(px(4.0))
                                    .rounded_full()
                                    .bg(self.tint(0xffffff1a, 0x00000015)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(13.0))
                                    .w(relative((cps - 1) as f32 / 29.0))
                                    .h(px(4.0))
                                    .rounded_full()
                                    .bg(rgb(self.accent())),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(7.0))
                                    .left(relative((cps - 1) as f32 / 29.0))
                                    .ml(px(-8.0))
                                    .size(px(16.0))
                                    .rounded_full()
                                    .bg(rgb(self.accent())),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .mt(px(4.0))
                            .text_size(px(10.0))
                            .text_color(rgb(self.faint()))
                            .child("1 CPS")
                            .child("30 CPS"),
                    ),
            )
            .into_any_element()
    }
    fn settings_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let snapshot = self.engine.snapshot();
        let capturing = snapshot.capturing_key;
        self.panel()
            .px(px(20.0))
            .child(
                self.row(
                    "Toggle key",
                    if capturing {
                        "Press a key. F10 is reserved for stop."
                    } else {
                        "Press Change, then any key."
                    },
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(self.keycap(snapshot.toggle_key.label()))
                        .child(
                            div()
                                .id("capture-key")
                                .px(px(12.0))
                                .py(px(7.0))
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .bg(if capturing {
                                    self.accent_fill(26)
                                } else {
                                    self.tint(0xffffff0c, 0x00000005)
                                })
                                .text_color(rgb(if capturing {
                                    self.accent()
                                } else {
                                    self.text()
                                }))
                                .hover(|d| d.bg(self.tint(0xffffff16, 0x0000000b)))
                                .on_click(cx.listener(|s, _, _, cx| {
                                    if s.startup_error.is_some() {
                                        return;
                                    }
                                    if s.engine.snapshot().capturing_key {
                                        s.engine.cancel_key_capture();
                                    } else {
                                        s.engine.begin_key_capture();
                                    }
                                    cx.notify();
                                }))
                                .child(if capturing { "Cancel" } else { "Change" }),
                        )
                        .into_any_element(),
                ),
            )
            .child(self.divider())
            .child(
                self.row(
                    "Dark mode",
                    "Use a dark appearance.",
                    div()
                        .id("dark-mode-setting")
                        .cursor_pointer()
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.preferences.dark_mode = !s.preferences.dark_mode;
                            s.save();
                            cx.notify();
                        }))
                        .child(self.switch(self.preferences.dark_mode))
                        .into_any_element(),
                ),
            )
            .child(self.divider())
            .child(
                self.row(
                    "Minimize on enable",
                    "Minimize when the clicker is enabled.",
                    div()
                        .id("minimize-setting")
                        .cursor_pointer()
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.preferences.minimize_on_enable = !s.preferences.minimize_on_enable;
                            s.save();
                            cx.notify();
                        }))
                        .child(self.switch(self.preferences.minimize_on_enable))
                        .into_any_element(),
                ),
            )
            .into_any_element()
    }
    fn header(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        div()
            .h(px(56.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .pr(px(16.0))
            .child(
                div()
                    .flex_1()
                    .h(px(48.0))
                    .pl(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    // Four painted pixels above this area preserve top resizing.
                    .window_control_area(WindowControlArea::Drag)
                    .child(Self::icon("mark.svg", self.accent(), 15.0))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Cinnabar clicker"),
                    ),
            )
            .child(
                div()
                    .h(px(34.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .id("minimize")
                            .w(px(40.0))
                            .h_full()
                            .rounded(px(7.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|d| d.bg(self.tint(0xffffff0e, 0x00000006)))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(|_, window, _| window.minimize_window())
                            .child(Self::icon("minimize.svg", self.muted(), 15.0)),
                    )
                    .child(
                        div()
                            .id("maximize")
                            .w(px(40.0))
                            .h_full()
                            .rounded(px(7.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|d| d.bg(self.tint(0xffffff0e, 0x00000006)))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(|_, window, _| window.zoom_window())
                            .child(Self::icon(
                                if window.is_maximized() {
                                    "restore.svg"
                                } else {
                                    "maximize.svg"
                                },
                                self.muted(),
                                15.0,
                            )),
                    )
                    .child(
                        div()
                            .id("close")
                            .w(px(40.0))
                            .h_full()
                            .rounded(px(7.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|d| d.bg(self.tint(0xe7615230, 0xe7615219)))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.engine.stop();
                                cx.quit();
                            }))
                            .child(Self::icon("close.svg", self.muted(), 15.0)),
                    ),
            )
    }
}
impl Render for Client {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.engine.snapshot();
        let status = if self.startup_error.is_some() {
            "Unavailable"
        } else if snapshot.input_error.is_some() {
            "Input blocked"
        } else if snapshot.clicking {
            "Clicking"
        } else if snapshot.enabled && !snapshot.physical_left_down {
            "Ready"
        } else if snapshot.enabled && !snapshot.foreground_allowed {
            "Paused"
        } else if snapshot.enabled {
            "Ready"
        } else {
            "Disabled"
        };
        let binding_error = if snapshot.capturing_key {
            snapshot.hotkey_error.clone().or(snapshot.last_error)
        } else {
            snapshot.last_error
        };
        let error = self
            .startup_error
            .clone()
            .or(binding_error)
            .or(self.error.clone());
        let content = match self.page {
            Page::Clicker => self.clicker_page(cx),
            Page::Settings => self.settings_page(cx),
        };
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(self.tint(0x181818fa, 0xf4f4f4ed))
            .text_color(rgb(self.text()))
            .font_family("Segoe UI Variable")
            .text_size(px(12.0))
            .flex()
            .flex_col()
            .on_mouse_move(cx.listener(|s, e: &MouseMoveEvent, _, cx| {
                if s.dragging && e.pressed_button == Some(MouseButton::Left) {
                    s.set_slider(e.position.x, cx);
                }
                if e.pressed_button.is_none() {
                    s.dragging = false;
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, _| s.dragging = false),
            )
            .child(self.header(window, cx))
            .child(
                div()
                    .px(px(24.0))
                    .h(px(44.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(self.tab("Clicker", Page::Clicker, cx))
                    .child(self.tab("Preferences", Page::Settings, cx)),
            )
            .child(
                div()
                    .id("page-content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(24.0))
                    .pt(px(16.0))
                    .pb(px(20.0))
                    .child(content)
                    .when_some(error, |d, message| {
                        d.child(
                            div()
                                .mt(px(12.0))
                                .p(px(12.0))
                                .rounded(px(8.0))
                                .bg(self.tint(0x402522f0, 0xffede4e0))
                                .text_size(px(11.0))
                                .text_color(rgb(self.tone(0xf0aaa0, 0xa23c2d)))
                                .child(message),
                        )
                    }),
            )
            .child(
                div()
                    .h(px(36.0))
                    .flex_shrink_0()
                    .px(px(26.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(10.0))
                    .text_color(rgb(self.muted()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .child(div().size(px(5.0)).rounded_full().bg(rgb(
                                if snapshot.enabled {
                                    self.accent()
                                } else {
                                    self.faint()
                                },
                            )))
                            .child(status),
                    )
                    .child(format!(
                        "{} toggle · {}",
                        snapshot
                            .active_toggle_key
                            .map_or_else(|| snapshot.toggle_key.label(), |key| key.label()),
                        if snapshot.hotkeys_available || self.preview {
                            "F10 stop"
                        } else {
                            "Hotkeys unavailable"
                        }
                    )),
            )
    }
}
fn main() {
    let preview = std::env::args().any(|arg| arg == "--smoke-test");
    let page = if preview && std::env::args().any(|arg| arg == "--page=settings") {
        Page::Settings
    } else {
        Page::Clicker
    };
    let light = preview && std::env::args().any(|arg| arg == "--light");
    Application::new()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            let bounds = Bounds::centered(None, size(px(680.0), px(480.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(600.0), px(450.0))),
                    window_background: WindowBackgroundAppearance::Blurred,
                    titlebar: Some(TitlebarOptions {
                        title: Some("Cinnabar clicker".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    app_id: Some("cinnabar-clicker".into()),
                    ..Default::default()
                },
                |window, cx| {
                    window_frame::configure(window);
                    window.on_window_should_close(cx, |_, cx| {
                        cx.quit();
                        true
                    });
                    cx.new(|cx| {
                        let mut client = Client::new(preview, page, window, cx);
                        if light {
                            client.preferences.dark_mode = false;
                        }
                        client
                    })
                },
            )
            .expect("Could not open Cinnabar clicker window");
            cx.activate(true);
            if preview {
                cx.spawn(async move |cx| {
                    Timer::after(Duration::from_secs(3)).await;
                    let _ = cx.update(|cx| cx.quit());
                })
                .detach();
            }
        });
}

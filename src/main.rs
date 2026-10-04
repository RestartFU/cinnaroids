#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod client_process;
mod component;
mod settings;
mod window_frame;

use component::{AttachRequest, ModComponent};
use gpui::{prelude::*, *};
use settings::Settings;
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const PRODUCT_NAME: &str = "Cinnaroids";

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExecutableRevision {
    size: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClientWatch {
    executable: PathBuf,
    revision: Option<ExecutableRevision>,
}

impl ClientWatch {
    fn observe(&mut self, revision: Option<ExecutableRevision>) -> bool {
        if self.revision == revision {
            return false;
        }
        self.revision = revision;
        // A temporarily absent executable is not a candidate for attachment.
        self.revision.is_some()
    }
}

fn executable_revision(path: &Path) -> Option<ExecutableRevision> {
    let metadata = std::fs::metadata(path).ok()?;
    metadata.is_file().then(|| ExecutableRevision {
        size: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

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

struct Launcher {
    component: Option<ModComponent>,
    preferences: Settings,
    error: Option<String>,
    status: String,
    preview: bool,
    attachment: Option<AttachRequest>,
    pending: bool,
    client_watch: Option<ClientWatch>,
}

impl Launcher {
    fn new(preview: bool) -> Self {
        let (preferences, mut error) = if preview {
            (Settings::default(), None)
        } else {
            Settings::load()
        };
        let component = if preview {
            None
        } else {
            match ModComponent::new() {
                Ok(component) => Some(component),
                Err(message) => {
                    error = Some(message);
                    None
                }
            }
        };
        Self {
            component,
            preferences,
            error,
            status: "Ready".into(),
            preview,
            attachment: None,
            pending: false,
            client_watch: None,
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
    fn accent(&self) -> u32 {
        self.tone(0xec8273, 0xdb6555)
    }
    fn icon(path: &'static str, color: u32, size: f32) -> Svg {
        svg().path(path).size(px(size)).text_color(rgb(color))
    }

    fn save(&mut self) {
        if !self.preview {
            self.error = self.preferences.save().err();
        }
    }

    fn selected_client(&self) -> Option<PathBuf> {
        let saved = self
            .preferences
            .cinnabar_path
            .clone()
            .filter(|path| path.is_file());
        let bundled = (|| {
            std::env::current_exe()
                .ok()?
                .parent()
                .map(|directory| directory.join("Cinnabar/bedrock-client.exe"))
                .filter(|path| path.is_file())
        })();
        settings::select_client(saved, bundled)
    }

    fn attach(&mut self, executable: PathBuf, start_if_absent: bool, cx: &mut Context<Self>) {
        if self.pending {
            return;
        }
        let Some(component) = self.component.clone() else {
            return;
        };
        self.pending = true;
        self.status = "Attaching…".into();
        self.error = None;
        self.attachment = None;
        cx.notify();
        let watched_executable = executable.clone();
        let task = cx.background_executor().spawn(async move {
            let revision = executable_revision(&executable);
            (revision, component.attach(&executable, start_if_absent))
        });
        cx.spawn(async move |view, cx| {
            let (revision, result) = task.await;
            let _ = view.update(cx, |s, cx| {
                s.pending = false;
                s.client_watch = Some(ClientWatch {
                    executable: watched_executable,
                    revision,
                });
                match result {
                    Ok(request) => {
                        s.preferences.cinnabar_path = Some(request.executable.clone());
                        s.save();
                        s.status = if request.client_pid.is_some() {
                            "Waiting for Cinnabar to load modules".into()
                        } else {
                            "Ready — start Cinnabar".into()
                        };
                        s.attachment = Some(request);
                    }
                    Err(error) => {
                        s.error = Some(error);
                        s.status = "Attachment unavailable".into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview || self.component.is_none() {
            return;
        }
        if let Some(executable) = self.selected_client() {
            self.attach(executable, true, cx);
        } else {
            self.choose(true, window, cx);
        }
    }

    fn choose(&mut self, launch: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview || self.pending {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Cinnabar's bedrock-client.exe".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = paths.await;
            let _ = view.update(cx, |s, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        if launch {
                            s.attach(path, true, cx);
                        } else {
                            s.preferences.cinnabar_path = Some(path);
                            s.save();
                            if let Some(executable) = s.selected_client() {
                                s.attach(executable, false, cx);
                            }
                        }
                    }
                }
                Ok(Ok(None)) => {}
                _ => {
                    s.error = Some("Could not select Cinnabar.".into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn switch(&self) -> Div {
        div()
            .w(px(40.0))
            .h(px(23.0))
            .rounded_full()
            .bg(if self.preferences.dark_mode {
                rgb(self.accent())
            } else {
                self.tint(0xffffff26, 0x00000025)
            })
            .flex()
            .items_center()
            .px(px(3.0))
            .when(self.preferences.dark_mode, |d| d.justify_end())
            .child(div().size(px(17.0)).rounded_full().bg(rgb(0xffffff)))
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
                    .window_control_area(WindowControlArea::Drag)
                    .child(Self::icon("mark.svg", self.accent(), 15.0))
                    .child(div().font_weight(FontWeight::MEDIUM).child(PRODUCT_NAME)),
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
                            .on_click(cx.listener(|_, _, _, cx| cx.quit()))
                            .child(Self::icon("close.svg", self.muted(), 15.0)),
                    ),
            )
    }
}

impl Render for Launcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let client_label = if self.selected_client().is_some() {
            "Cinnabar"
        } else {
            "Select your installed Cinnabar"
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
            .child(self.header(window, cx))
            .child(
                div()
                    .id("launcher-content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(24.0))
                    .pt(px(16.0))
                    .pb(px(20.0))
                    .child(
                        div()
                            .p(px(22.0))
                            .rounded(px(12.0))
                            .bg(self.tint(0x242424fa, 0xfffffff0))
                            .border_1()
                            .border_color(self.tint(0xffffff13, 0x0000000d))
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_size(px(15.0))
                                    .child(client_label),
                            )
                            .child(div().mt(px(8.0)).text_color(rgb(self.muted())).child(
                                "Attaches automatically. Right Shift opens modules in-game.",
                            ))
                            .child(
                                div()
                                    .mt(px(22.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.0))
                                    .child(
                                        div()
                                            .id("start-cinnabar")
                                            .px(px(16.0))
                                            .py(px(10.0))
                                            .rounded(px(7.0))
                                            .cursor_pointer()
                                            .bg(rgb(self.accent()))
                                            .text_color(rgb(0x181818))
                                            .hover(|d| d.opacity(0.85))
                                            .on_click(cx.listener(Self::start))
                                            .child(if self.status.starts_with("Attached") {
                                                "Attach"
                                            } else {
                                                "Start Cinnabar"
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id("choose-client")
                                            .px(px(14.0))
                                            .py(px(10.0))
                                            .rounded(px(7.0))
                                            .cursor_pointer()
                                            .bg(self.tint(0xffffff0b, 0x00000005))
                                            .hover(|d| d.bg(self.tint(0xffffff16, 0x0000000b)))
                                            .on_click(cx.listener(|s, _, window, cx| {
                                                s.choose(false, window, cx)
                                            }))
                                            .child("Choose client"),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(16.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(2.0))
                            .child(div().text_color(rgb(self.muted())).child("Dark mode"))
                            .child(
                                div()
                                    .id("dark-mode")
                                    .cursor_pointer()
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.preferences.dark_mode = !s.preferences.dark_mode;
                                        s.save();
                                        cx.notify();
                                    }))
                                    .child(self.switch()),
                            ),
                    )
                    .when_some(self.error.clone(), |d, message| {
                        d.child(
                            div()
                                .mt(px(14.0))
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
                    .child(self.status.clone())
                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
            )
    }
}

fn main() {
    let preview = std::env::args().any(|arg| arg == "--smoke-test");
    let background = std::env::args().any(|arg| arg == "--background");
    let light = preview && std::env::args().any(|arg| arg == "--light");
    Application::new()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            let bounds = Bounds::centered(None, size(px(600.0), px(360.0)), cx);
            cx.open_window(
                WindowOptions {
                    focus: !background,
                    show: !background,
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(500.0), px(340.0))),
                    window_background: WindowBackgroundAppearance::Blurred,
                    titlebar: Some(TitlebarOptions {
                        title: Some(PRODUCT_NAME.into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    app_id: Some("cinnaroids".into()),
                    ..Default::default()
                },
                |window, cx| {
                    window_frame::configure(window);
                    window.on_window_should_close(cx, |_, cx| {
                        cx.quit();
                        true
                    });
                    cx.new(|cx| {
                        let mut launcher = Launcher::new(preview);
                        if light {
                            launcher.preferences.dark_mode = false;
                        }
                        if !preview {
                            if let Some(executable) = launcher.selected_client() {
                                launcher.attach(executable, false, cx);
                            }
                            cx.spawn(async move |view, cx| {
                                loop {
                                    Timer::after(Duration::from_millis(500)).await;
                                    let (request, watch, pending) =
                                        match view.read_with(cx, |s, _| {
                                            (
                                                s.attachment.clone(),
                                                s.client_watch.clone(),
                                                s.pending,
                                            )
                                        }) {
                                            Ok(snapshot) => snapshot,
                                            Err(_) => break,
                                        };
                                    if pending || (request.is_none() && watch.is_none()) {
                                        continue;
                                    }
                                    let watched_executable =
                                        watch.as_ref().map(|watch| watch.executable.clone());
                                    let (revision, status) = cx
                                        .background_executor()
                                        .spawn(async move {
                                            let revision = watched_executable
                                                .as_deref()
                                                .and_then(executable_revision);
                                            let status = request.map(|request| {
                                                (request.id.clone(), request.status())
                                            });
                                            (revision, status)
                                        })
                                        .await;
                                    if view
                                        .update(cx, |s, cx| {
                                            if !s.pending
                                                && watch.is_some()
                                                && s.client_watch == watch
                                            {
                                                let updated_executable =
                                                    s.client_watch.as_mut().and_then(|watch| {
                                                        watch
                                                            .observe(revision)
                                                            .then(|| watch.executable.clone())
                                                    });
                                                if let Some(executable) = updated_executable {
                                                    s.attach(executable, false, cx);
                                                    return;
                                                }
                                            }
                                            let Some((request_id, result)) = status else {
                                                return;
                                            };
                                            if s.attachment
                                                .as_ref()
                                                .is_none_or(|request| request.id != request_id)
                                            {
                                                return;
                                            }
                                            match result {
                                                Ok(Some(status)) if s.status != status => {
                                                    s.status = status;
                                                    s.error = None;
                                                    cx.notify();
                                                }
                                                Err(error) if s.error.as_ref() != Some(&error) => {
                                                    s.error = Some(error);
                                                    s.status = "Could not attach".into();
                                                    cx.notify();
                                                }
                                                _ => {}
                                            }
                                        })
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            })
                            .detach();
                        }
                        launcher
                    })
                },
            )
            .expect("Could not open Cinnaroids window");
            if !preview && !background {
                cx.activate(true);
            }
            if preview {
                cx.spawn(async move |cx| {
                    Timer::after(Duration::from_secs(3)).await;
                    let _ = cx.update(|cx| cx.quit());
                })
                .detach();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn client_update_retries_once_and_waits_through_a_missing_executable() {
        let original = ExecutableRevision {
            size: 100,
            modified: Some(SystemTime::UNIX_EPOCH),
        };
        let updated = ExecutableRevision {
            size: 110,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1)),
        };
        let mut watch = ClientWatch {
            executable: "C:/Installed/Cinnabar/bedrock-client.exe".into(),
            revision: Some(original.clone()),
        };
        for _ in 0..100 {
            assert!(!watch.observe(Some(original.clone())));
        }
        assert!(watch.observe(Some(updated.clone())));
        assert!(!watch.observe(Some(updated.clone())));
        let rebuilt = ExecutableRevision {
            size: updated.size,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(2)),
        };
        assert!(watch.observe(Some(rebuilt.clone())));
        assert!(!watch.observe(Some(rebuilt.clone())));
        assert!(!watch.observe(None));
        assert!(!watch.observe(None));
        assert!(watch.observe(Some(rebuilt.clone())));
        assert!(!watch.observe(Some(rebuilt)));
    }
}

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(windows)]
mod client_process;
#[cfg(unix)]
#[path = "client_process_unix.rs"]
mod client_process;
mod component;
mod settings;
#[cfg(windows)]
mod window_frame;
#[cfg(unix)]
mod window_frame {
    pub fn configure(_: &gpui::Window) {}
}

use component::{AttachRequest, ModComponent};
use gpui::{prelude::*, *};
use settings::Settings;
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

const PRODUCT_NAME: &str = "Cinnaroids";
const ATTACH_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
];

#[derive(Default)]
struct AttachmentRetry {
    retries: usize,
    next_attempt: Option<Instant>,
}

impl AttachmentRetry {
    fn failed(&mut self, now: Instant) {
        if self.next_attempt.is_none()
            && let Some(delay) = ATTACH_RETRY_DELAYS.get(self.retries)
        {
            self.retries += 1;
            self.next_attempt = Some(now + *delay);
        }
    }

    fn take_due(&mut self, now: Instant) -> bool {
        if self.next_attempt.is_some_and(|deadline| now >= deadline) {
            self.next_attempt = None;
            true
        } else {
            false
        }
    }

    fn registered(&mut self) {
        // Registration success stops local retries; a failed host ACK keeps this budget.
        self.next_attempt = None;
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExecutableRevision {
    size: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClientWatch {
    executable: PathBuf,
    revision: Option<ExecutableRevision>,
    client_pid: Option<u32>,
}

impl ClientWatch {
    fn observe(&mut self, revision: Option<ExecutableRevision>, client_pid: Option<u32>) -> bool {
        if self.revision == revision && self.client_pid == client_pid {
            return false;
        }
        self.revision = revision;
        self.client_pid = client_pid;
        // A temporarily absent executable is not a candidate for attachment.
        self.revision.is_some()
    }
}

fn observe_installed_client() -> Result<Option<ClientWatch>, String> {
    let Some(executable) = settings::installed_client_path() else {
        return Ok(None);
    };
    let revision = executable_revision(&executable);
    let client_pid = if revision.is_some() {
        client_process::running_client(&executable)?
    } else {
        None
    };
    Ok(Some(ClientWatch {
        executable,
        revision,
        client_pid,
    }))
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
    retry: AttachmentRetry,
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
            retry: AttachmentRetry::default(),
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

    fn attach(&mut self, executable: PathBuf, cx: &mut Context<Self>) {
        if self.pending || self.preview {
            return;
        }
        let component = self.component.clone();
        self.pending = true;
        self.status = "Attaching…".into();
        self.attachment = None;
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            match component.map_or_else(ModComponent::new, Ok) {
                Ok(component) => {
                    let result = component.attach(&executable);
                    (Some(component), result)
                }
                Err(error) => (None, Err(error)),
            }
        });
        cx.spawn(async move |view, cx| {
            let (component, result) = task.await;
            let _ = view.update(cx, |s, cx| {
                s.pending = false;
                s.component = component;
                match result {
                    Ok(request) => {
                        s.retry.registered();
                        s.status = if request.client_pid.is_some() {
                            "Waiting for Cinnabar to load modules".into()
                        } else {
                            s.error = None;
                            "Waiting for Cinnabar".into()
                        };
                        s.attachment = Some(request);
                    }
                    Err(error) => {
                        if error == component::UNSUPPORTED_CLIENT {
                            s.retry.reset();
                            s.status = "Cinnabar detected — modules unavailable".into();
                        } else {
                            s.retry.failed(Instant::now());
                            s.status = "Attachment unavailable".into();
                        }
                        s.error = Some(error);
                    }
                }
                cx.notify();
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
                    .when(cfg!(target_os = "linux"), |titlebar| {
                        titlebar.on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.start_window_move();
                            cx.stop_propagation();
                        })
                    })
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
                                    .child("Cinnabar"),
                            )
                            .child(div().mt(px(8.0)).text_color(rgb(self.muted())).child(
                                "Attaches automatically. Right Shift opens modules in-game.",
                            )),
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
            if !preview {
                cx.on_app_quit(|_| {
                    if let Err(error) = component::disable_on_exit() {
                        eprintln!("{error}");
                    }
                    std::future::ready(())
                })
                .detach();
            }
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
                    cx.new(|cx: &mut Context<Launcher>| {
                        let mut launcher = Launcher::new(preview);
                        if light {
                            launcher.preferences.dark_mode = false;
                        }
                        if !preview {
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
                                    if pending {
                                        continue;
                                    }
                                    let (observation, status) = cx
                                        .background_executor()
                                        .spawn(async move {
                                            let observation = observe_installed_client();
                                            let status = request.map(|request| {
                                                (request.id.clone(), request.status())
                                            });
                                            (observation, status)
                                        })
                                        .await;
                                    if view
                                        .update(cx, |s, cx| {
                                            if s.pending || s.client_watch != watch {
                                                return;
                                            }
                                            let observed = match observation {
                                                Ok(Some(observed)) => observed,
                                                Ok(None) => {
                                                    s.error =
                                                        Some("Settings folder unavailable.".into());
                                                    s.status = "Attachment unavailable".into();
                                                    cx.notify();
                                                    return;
                                                }
                                                Err(error) => {
                                                    if s.error.as_ref() != Some(&error) {
                                                        s.error = Some(error);
                                                        s.status =
                                                            "Could not inspect Cinnabar".into();
                                                        cx.notify();
                                                    }
                                                    return;
                                                }
                                            };
                                            let executable = observed.executable.clone();
                                            let installed = observed.revision.is_some();
                                            let should_attach = match &mut s.client_watch {
                                                Some(watch) if watch.executable == executable => {
                                                    watch.observe(
                                                        observed.revision,
                                                        observed.client_pid,
                                                    )
                                                }
                                                _ => {
                                                    s.client_watch = Some(observed);
                                                    installed
                                                }
                                            };
                                            if should_attach {
                                                s.retry.reset();
                                                s.attach(executable, cx);
                                                return;
                                            }
                                            if !installed {
                                                s.retry.reset();
                                                s.attachment = None;
                                                if s.status != "Waiting for installed Cinnabar" {
                                                    s.status =
                                                        "Waiting for installed Cinnabar".into();
                                                    cx.notify();
                                                }
                                                return;
                                            }
                                            if let Some((request_id, result)) = status
                                                && s.attachment
                                                    .as_ref()
                                                    .is_some_and(|request| request.id == request_id)
                                            {
                                                match result {
                                                    Ok(Some(status)) => {
                                                        let attached =
                                                            status.starts_with("Attached");
                                                        if attached {
                                                            s.retry.reset();
                                                        }
                                                        if s.status != status
                                                            || (attached && s.error.is_some())
                                                        {
                                                            s.status = status;
                                                            if attached {
                                                                s.error = None;
                                                            }
                                                            cx.notify();
                                                        }
                                                    }
                                                    Err(error) => {
                                                        s.retry.failed(Instant::now());
                                                        if s.error.as_ref() != Some(&error)
                                                            || s.status != "Could not attach"
                                                        {
                                                            s.error = Some(error);
                                                            s.status = "Could not attach".into();
                                                            cx.notify();
                                                        }
                                                    }
                                                    _ => {}
                                                }
                                            }
                                            if s.retry.take_due(Instant::now()) {
                                                s.attach(executable, cx);
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
    fn transient_attachment_failure_recovers_without_a_new_observation_or_success_churn() {
        let revision = ExecutableRevision {
            size: 100,
            modified: Some(SystemTime::UNIX_EPOCH),
        };
        let mut watch = ClientWatch {
            executable: "C:/Installed/Cinnabar/bedrock-client.exe".into(),
            revision: Some(revision.clone()),
            client_pid: Some(100),
        };
        let mut retry = AttachmentRetry::default();
        let failed_at = Instant::now();
        retry.failed(failed_at);
        assert!(!watch.observe(Some(revision.clone()), Some(100)));
        assert!(!retry.take_due(failed_at + Duration::from_secs(1)));
        assert!(retry.take_due(failed_at + Duration::from_secs(2)));
        retry.registered();
        for seconds in 2..120 {
            assert!(!watch.observe(Some(revision.clone()), Some(100)));
            assert!(!retry.take_due(failed_at + Duration::from_secs(seconds)));
        }
    }

    #[test]
    fn host_failures_keep_the_bounded_budget_until_a_confirmed_attachment() {
        let mut retry = AttachmentRetry::default();
        let mut now = Instant::now();
        for delay in ATTACH_RETRY_DELAYS {
            retry.failed(now);
            // Repeated observations of the same failure must not postpone or multiply retries.
            retry.failed(now + Duration::from_millis(500));
            assert!(!retry.take_due(now + delay - Duration::from_millis(1)));
            assert!(retry.take_due(now + delay));
            retry.registered();
            now += delay;
        }
        retry.failed(now);
        assert!(!retry.take_due(now + Duration::from_secs(60)));
        retry.reset();
        retry.failed(now);
        assert!(retry.take_due(now + Duration::from_secs(2)));
    }

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
            client_pid: None,
        };
        for _ in 0..100 {
            assert!(!watch.observe(Some(original.clone()), None));
        }
        assert!(watch.observe(Some(updated.clone()), None));
        assert!(!watch.observe(Some(updated.clone()), None));
        let rebuilt = ExecutableRevision {
            size: updated.size,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(2)),
        };
        assert!(watch.observe(Some(rebuilt.clone()), None));
        assert!(!watch.observe(Some(rebuilt.clone()), None));
        assert!(!watch.observe(None, None));
        assert!(!watch.observe(None, None));
        assert!(watch.observe(Some(rebuilt.clone()), None));
        assert!(!watch.observe(Some(rebuilt), None));
    }

    #[test]
    fn automatic_attachment_tracks_later_installation_and_game_restarts_once() {
        let revision = ExecutableRevision {
            size: 100,
            modified: Some(SystemTime::UNIX_EPOCH),
        };
        let mut watch = ClientWatch {
            executable: "C:/Installed/Cinnabar/bedrock-client.exe".into(),
            revision: None,
            client_pid: None,
        };
        for _ in 0..100 {
            assert!(!watch.observe(None, None));
        }
        assert!(watch.observe(Some(revision.clone()), None));
        for _ in 0..100 {
            assert!(!watch.observe(Some(revision.clone()), None));
        }
        assert!(watch.observe(Some(revision.clone()), Some(100)));
        for _ in 0..100 {
            assert!(!watch.observe(Some(revision.clone()), Some(100)));
        }
        assert!(watch.observe(Some(revision.clone()), None));
        assert!(!watch.observe(Some(revision.clone()), None));
        assert!(watch.observe(Some(revision.clone()), Some(200)));
        assert!(!watch.observe(Some(revision), Some(200)));
    }
}

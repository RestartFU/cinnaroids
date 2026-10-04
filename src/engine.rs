//! System-wide Windows clicker. Synthetic input never changes the physical hold state.
//!
//! The mouse/keyboard hooks and F10 emergency stop share a continuously pumped thread.
//! The worker uses a monotonic clock, releases before each generated down edge,
//! and never catches up with a burst after the machine or target stalls.

use std::{
    cell::RefCell,
    mem::size_of,
    ptr::null_mut,
    sync::{Arc, Condvar, Mutex, MutexGuard, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    Foundation::{GetLastError, LPARAM, LRESULT, SetLastError, WPARAM},
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{GetCurrentProcessId, GetCurrentThreadId},
    },
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetKeyNameTextW, INPUT, INPUT_0, INPUT_MOUSE, MAPVK_VK_TO_VSC_EX,
            MOD_NOREPEAT, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, MapVirtualKeyW,
            RegisterHotKey, SendInput, UnregisterHotKey, VK_F10,
        },
        WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW,
            GetWindowThreadProcessId, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLKHF_LOWER_IL_INJECTED,
            LLMHF_INJECTED, LLMHF_LOWER_IL_INJECTED, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE,
            PeekMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
            UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP,
            WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
        },
    },
};

const POLL_INTERVAL: Duration = Duration::from_millis(5);
const STOP_HOTKEY_ID: i32 = 2;
const INPUT_MARKER: usize = 0x4349_4E4E;

/// A keyboard virtual key. Mouse buttons and reserved F10 cannot become the toggle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToggleKey(u32);

impl Default for ToggleKey {
    fn default() -> Self {
        Self::F8
    }
}

impl ToggleKey {
    pub const F8: Self = Self(0x77);

    pub fn label(self) -> String {
        match self.0 {
            0x30..=0x39 | 0x41..=0x5A => char::from_u32(self.0).unwrap().to_string(),
            0x70..=0x87 => format!("F{}", self.0 - 0x6F),
            0x08 => "Backspace".into(),
            0x09 => "Tab".into(),
            0x0D => "Enter".into(),
            0x10 => "Shift".into(),
            0x11 => "Ctrl".into(),
            0x12 => "Alt".into(),
            0x13 => "Pause".into(),
            0x14 => "Caps Lock".into(),
            0x1B => "Escape".into(),
            0x20 => "Space".into(),
            0x21 => "Page Up".into(),
            0x22 => "Page Down".into(),
            0x23 => "End".into(),
            0x24 => "Home".into(),
            0x25 => "Left".into(),
            0x26 => "Up".into(),
            0x27 => "Right".into(),
            0x28 => "Down".into(),
            0x2C => "Print Screen".into(),
            0x2D => "Insert".into(),
            0x2E => "Delete".into(),
            0x5B => "Left Win".into(),
            0x5C => "Right Win".into(),
            0x5D => "Menu".into(),
            0x60..=0x69 => format!("Numpad {}", self.0 - 0x60),
            0x6A => "Numpad *".into(),
            0x6B => "Numpad +".into(),
            0x6C => "Numpad Separator".into(),
            0x6D => "Numpad -".into(),
            0x6E => "Numpad .".into(),
            0x6F => "Numpad /".into(),
            0x90 => "Num Lock".into(),
            0x91 => "Scroll Lock".into(),
            0xA0 => "Left Shift".into(),
            0xA1 => "Right Shift".into(),
            0xA2 => "Left Ctrl".into(),
            0xA3 => "Right Ctrl".into(),
            0xA4 => "Left Alt".into(),
            0xA5 => "Right Alt".into(),
            _ => {
                // OEM punctuation and less common keyboard keys use the current layout's name.
                let scan = unsafe { MapVirtualKeyW(self.0, MAPVK_VK_TO_VSC_EX) };
                let extended = (scan & 0xFF00 != 0) as i32;
                let lparam = (((scan & 0xFF) as i32) << 16) | (extended << 24);
                let mut name = [0u16; 64];
                let length =
                    unsafe { GetKeyNameTextW(lparam, name.as_mut_ptr(), name.len() as i32) };
                if length > 0 {
                    String::from_utf16_lossy(&name[..length as usize])
                } else {
                    format!("Key 0x{:02X}", self.0)
                }
            }
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        let label = label.trim();
        if let Some(value) = label.to_ascii_uppercase().strip_prefix("VK:") {
            return value.parse().ok().and_then(Self::from_virtual_key);
        }
        if let Some(number) = label.to_ascii_uppercase().strip_prefix('F')
            && let Ok(number) = number.parse::<u32>()
            && (1..=24).contains(&number)
        {
            return Self::from_virtual_key(0x6F + number);
        }
        if label.len() == 1 {
            let value = label.as_bytes()[0].to_ascii_uppercase();
            if value.is_ascii_alphanumeric() {
                return Self::from_virtual_key(value as u32);
            }
        }
        (1..=254)
            .filter_map(Self::from_virtual_key)
            .find(|key| key.label().eq_ignore_ascii_case(label))
    }

    pub const fn virtual_key(self) -> u32 {
        self.0
    }

    pub fn from_virtual_key(key: u32) -> Option<Self> {
        if (1..=254).contains(&key) && !matches!(key, 0x01 | 0x02 | 0x04 | 0x05 | 0x06 | 0x79) {
            Some(Self(key))
        } else {
            None
        }
    }

    /// Layout-independent setting value; the display label can change with keyboard layout.
    pub fn storage_label(self) -> String {
        format!("VK:{}", self.0)
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub enabled: bool,
    pub physical_left_down: bool,
    pub clicking: bool,
    pub foreground_allowed: bool,
    pub cps: u32,
    /// The selected physical keyboard key; F10 remains reserved for emergency stop.
    pub toggle_key: ToggleKey,
    pub active_toggle_key: Option<ToggleKey>,
    pub capturing_key: bool,
    /// Changes when a binding is set or captured, so the UI can save it immediately.
    pub binding_revision: u64,
    /// Changes on every emergency stop, including when clicking is already disabled.
    pub stop_revision: u64,
    pub hotkeys_available: bool,
    #[cfg(test)]
    pub clicks: u64,
    pub last_error: Option<String>,
    pub input_error: Option<String>,
    pub hotkey_error: Option<String>,
}

struct State {
    enabled: bool,
    physical_left_down: bool,
    clicking: bool,
    foreground_allowed: bool,
    cps: u32,
    toggle_key: ToggleKey,
    active_toggle_key: Option<ToggleKey>,
    capturing_key: bool,
    binding_revision: u64,
    stop_revision: u64,
    keys_down: [bool; 256],
    consumed_keys: [bool; 256],
    stop_registered: bool,
    clicks: u64,
    input_error: Option<String>,
    hotkey_error: Option<String>,
    stopping: bool,
    revision: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            enabled: false,
            physical_left_down: false,
            clicking: false,
            foreground_allowed: false,
            cps: 12,
            toggle_key: ToggleKey::default(),
            active_toggle_key: None,
            capturing_key: false,
            binding_revision: 0,
            stop_revision: 0,
            keys_down: [false; 256],
            consumed_keys: [false; 256],
            stop_registered: false,
            clicks: 0,
            input_error: None,
            hotkey_error: None,
            stopping: false,
            revision: 0,
        }
    }
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(test)]
    fn enabled(&self, enabled: bool) {
        let mut state = self.lock();
        if !state.stopping && (!enabled || !state.capturing_key) {
            state.enabled = enabled;
            state.revision = state.revision.wrapping_add(1);
            if enabled {
                state.input_error = None;
            }
        }
        drop(state);
        self.wake.notify_all();
    }

    fn toggle(&self) {
        let mut state = self.lock();
        if !state.stopping && !state.capturing_key {
            state.enabled = !state.enabled;
            state.revision = state.revision.wrapping_add(1);
            if state.enabled {
                state.input_error = None;
            }
        }
        drop(state);
        self.wake.notify_all();
    }

    fn stop(&self) {
        let mut state = self.lock();
        state.enabled = false;
        state.revision = state.revision.wrapping_add(1);
        state.stop_revision = state.stop_revision.wrapping_add(1);
        drop(state);
        self.wake.notify_all();
    }

    fn observe_mouse(&self, message: u32, flags: u32) {
        if flags & (LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED) != 0 {
            return;
        }
        let held = match message {
            WM_LBUTTONDOWN => true,
            WM_LBUTTONUP => false,
            _ => return,
        };
        self.lock().physical_left_down = held;
        self.wake.notify_all();
    }

    /// Returns true only for a key event consumed by the binding picker. The keyboard
    /// hook calls this on physical events; tests feed it events without touching the desktop.
    fn observe_keyboard(&self, message: u32, virtual_key: u32, flags: u32) -> bool {
        if flags & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED) != 0 || virtual_key > 254 {
            return false;
        }
        let down = match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => true,
            WM_KEYUP | WM_SYSKEYUP => false,
            _ => return false,
        };
        let index = virtual_key as usize;
        let mut state = self.lock();
        let repeated = down && state.keys_down[index];
        state.keys_down[index] = down;
        if !down {
            let consumed = state.consumed_keys[index];
            state.consumed_keys[index] = false;
            return consumed;
        }
        if state.consumed_keys[index] {
            return true;
        }
        if repeated || state.stopping {
            return false;
        }
        let mut consumed = false;
        if virtual_key == VK_F10 as u32 {
            state.enabled = false;
            state.revision = state.revision.wrapping_add(1);
            state.stop_revision = state.stop_revision.wrapping_add(1);
            // Consume physical F10 so RegisterHotKey cannot enqueue a second,
            // delayed stop for this same press after the user re-enables aim.
            state.consumed_keys[index] = true;
            consumed = true;
            if state.capturing_key {
                state.hotkey_error =
                    Some("F10 is reserved for emergency stop. Press another key.".into());
            }
        } else if state.capturing_key {
            if let Some(key) = ToggleKey::from_virtual_key(virtual_key) {
                state.toggle_key = key;
                if state.active_toggle_key.is_some() {
                    state.active_toggle_key = Some(key);
                }
                state.binding_revision = state.binding_revision.wrapping_add(1);
                state.capturing_key = false;
                state.hotkey_error = None;
                // Consume the chosen key's repeats and keyup as well. It must be released
                // and pressed again before it can toggle, and it cannot trigger a UI action.
                state.consumed_keys[index] = true;
                consumed = true;
            }
        } else if state
            .active_toggle_key
            .is_some_and(|key| key.virtual_key() == virtual_key)
            || (state.active_toggle_key.is_some()
                && matches!(
                    (state.toggle_key.virtual_key(), virtual_key),
                    (0x10, 0xA0 | 0xA1) | (0x11, 0xA2 | 0xA3) | (0x12, 0xA4 | 0xA5)
                ))
        {
            state.enabled = !state.enabled;
            state.revision = state.revision.wrapping_add(1);
            if state.enabled {
                state.input_error = None;
            }
            // Reserve the selected toggle while this app is running. Passing Escape,
            // E, or another gameplay binding through could open a menu and release
            // the target's camera capture at the same moment the clicker toggles.
            state.consumed_keys[index] = true;
            consumed = true;
        }
        drop(state);
        self.wake.notify_all();
        consumed
    }
}

struct Inner {
    shared: Arc<Shared>,
    hook_thread_id: u32,
    hook_thread: Option<JoinHandle<()>>,
    worker_thread: Option<JoinHandle<()>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        {
            let mut state = self.shared.lock();
            state.enabled = false;
            state.stopping = true;
        }
        self.shared.wake.notify_all();
        // Keep the hook alive until the worker has released its last generated down.
        if let Some(worker) = self.worker_thread.take() {
            let _ = worker.join();
        }
        if self.hook_thread_id != 0 {
            // The startup handshake guarantees the thread already owns a message queue.
            unsafe { PostThreadMessageW(self.hook_thread_id, WM_QUIT, 0, 0) };
        }
        if let Some(hook) = self.hook_thread.take() {
            let _ = hook.join();
        }
    }
}

/// Cheaply cloneable engine handle. Dropping the final handle stops and joins both threads.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

impl Engine {
    pub fn new() -> Result<Self, String> {
        let shared = Shared::new();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let hook_shared = shared.clone();
        let hook_thread = thread::Builder::new()
            .name("cinnabar-input-hook".into())
            .spawn(move || hook_main(hook_shared, ready_tx))
            .map_err(|error| format!("Could not start the input hook thread: {error}"))?;
        let hook_thread_id = match ready_rx.recv() {
            Ok(Ok(thread_id)) => thread_id,
            Ok(Err(error)) => {
                let _ = hook_thread.join();
                return Err(error);
            }
            Err(error) => {
                let _ = hook_thread.join();
                return Err(format!("The input hook exited during startup: {error}"));
            }
        };
        let mut inner = Inner {
            shared: shared.clone(),
            hook_thread_id,
            hook_thread: Some(hook_thread),
            worker_thread: None,
        };
        {
            let state = inner.shared.lock();
            if !state.stop_registered {
                let detail = state
                    .hotkey_error
                    .as_deref()
                    .unwrap_or("F10 could not be registered");
                return Err(format!(
                    "The clicker could not start because its F10 emergency stop is unavailable. \
                     Close another clicker or application using F10, then reopen Cinnabar clicker. {detail}"
                ));
            }
        }
        inner.worker_thread = Some(
            thread::Builder::new()
                .name("cinnabar-click-worker".into())
                .spawn(move || worker_main(shared, WindowsBackend))
                .map_err(|error| format!("Could not start the click worker: {error}"))?,
        );
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    /// UI smoke-test mode: no hook, hotkeys, worker, or desktop input is installed.
    pub fn new_preview() -> Self {
        Self {
            inner: Arc::new(Inner {
                shared: Shared::new(),
                hook_thread_id: 0,
                hook_thread: None,
                worker_thread: None,
            }),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let state = self.inner.shared.lock();
        Snapshot {
            enabled: state.enabled,
            physical_left_down: state.physical_left_down,
            clicking: state.clicking,
            foreground_allowed: state.foreground_allowed,
            cps: state.cps,
            toggle_key: state.toggle_key,
            active_toggle_key: state.active_toggle_key,
            capturing_key: state.capturing_key,
            binding_revision: state.binding_revision,
            stop_revision: state.stop_revision,
            hotkeys_available: state.active_toggle_key.is_some() && state.stop_registered,
            #[cfg(test)]
            clicks: state.clicks,
            last_error: state
                .input_error
                .clone()
                .or_else(|| state.hotkey_error.clone()),
            input_error: state.input_error.clone(),
            hotkey_error: state.hotkey_error.clone(),
        }
    }

    pub fn set_cps(&self, cps: u32) {
        let mut state = self.inner.shared.lock();
        state.cps = cps.clamp(1, 30);
        state.revision = state.revision.wrapping_add(1);
        drop(state);
        self.inner.shared.wake.notify_all();
    }

    pub fn set_toggle_key(&self, key: ToggleKey) {
        let mut state = self.inner.shared.lock();
        state.toggle_key = key;
        if state.active_toggle_key.is_some() {
            state.active_toggle_key = Some(key);
        }
        state.capturing_key = false;
        state.binding_revision = state.binding_revision.wrapping_add(1);
    }

    pub fn begin_key_capture(&self) {
        let mut state = self.inner.shared.lock();
        if !state.stopping {
            state.enabled = false;
            state.capturing_key = true;
            state.hotkey_error = None;
            state.revision = state.revision.wrapping_add(1);
        }
        drop(state);
        self.inner.shared.wake.notify_all();
    }

    pub fn cancel_key_capture(&self) {
        let mut state = self.inner.shared.lock();
        state.capturing_key = false;
        // Cancellation also leaves the clicker disabled.
        state.hotkey_error = None;
    }

    #[cfg(test)]
    pub fn set_enabled(&self, enabled: bool) {
        self.inner.shared.enabled(enabled);
    }

    pub fn toggle(&self) {
        self.inner.shared.toggle();
    }

    pub fn stop(&self) {
        self.inner.shared.stop();
    }
}

thread_local! {
    static HOOK_CONTEXT: RefCell<Option<Arc<Shared>>> = const { RefCell::new(None) };
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam != 0 {
        // Windows guarantees a valid MSLLHOOKSTRUCT for nonnegative mouse hook codes.
        let event = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        // Avoid even taking the state lock for injected events, including our SendInput.
        if event.flags & (LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED) == 0 {
            HOOK_CONTEXT.with(|context| {
                if let Some(shared) = context.borrow().as_ref() {
                    shared.observe_mouse(wparam as u32, event.flags);
                }
            });
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam != 0 {
        // Windows guarantees KBDLLHOOKSTRUCT for nonnegative keyboard hook codes.
        let event = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        if event.flags & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED) == 0 {
            let consumed = HOOK_CONTEXT.with(|context| {
                context.borrow().as_ref().is_some_and(|shared| {
                    shared.observe_keyboard(wparam as u32, event.vkCode, event.flags)
                })
            });
            if consumed {
                return 1;
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

fn hook_main(shared: Arc<Shared>, ready: mpsc::SyncSender<Result<u32, String>>) {
    let thread_id = unsafe { GetCurrentThreadId() };
    let mut message = MSG::default();
    unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_NOREMOVE) };
    HOOK_CONTEXT.with(|context| *context.borrow_mut() = Some(shared.clone()));
    let mouse = unsafe {
        SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(mouse_hook),
            GetModuleHandleW(std::ptr::null()),
            0,
        )
    };
    if mouse.is_null() {
        let _ = ready.send(Err(win32_error(
            "Could not install the physical mouse hook",
        )));
        HOOK_CONTEXT.with(|context| *context.borrow_mut() = None);
        return;
    }
    let keyboard = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_hook),
            GetModuleHandleW(std::ptr::null()),
            0,
        )
    };
    if keyboard.is_null() {
        let error = win32_error("Could not install the physical keyboard hook");
        unsafe { UnhookWindowsHookEx(mouse) };
        HOOK_CONTEXT.with(|context| *context.borrow_mut() = None);
        let _ = ready.send(Err(error));
        return;
    }
    {
        let mut state = shared.lock();
        for key in 1..=254 {
            state.keys_down[key] = unsafe { GetAsyncKeyState(key as i32) } < 0;
        }
        state.active_toggle_key = Some(state.toggle_key);
        if unsafe { RegisterHotKey(null_mut(), STOP_HOTKEY_ID, MOD_NOREPEAT, VK_F10 as u32) } != 0 {
            state.stop_registered = true;
        } else {
            state.hotkey_error = Some(win32_error("F10 is unavailable"));
        }
    }
    if ready.send(Ok(thread_id)).is_err() {
        shared.lock().stopping = true;
    } else {
        loop {
            let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
            if result <= 0 {
                if result < 0 {
                    shared.lock().input_error = Some(win32_error("The input message loop failed"));
                }
                break;
            }
            if message.message == WM_HOTKEY {
                if message.wParam as i32 == STOP_HOTKEY_ID {
                    shared.stop();
                }
            } else {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
    }
    {
        let mut state = shared.lock();
        state.enabled = false;
        state.stopping = true;
        state.active_toggle_key = None;
        state.capturing_key = false;
        state.stop_registered = false;
    }
    shared.wake.notify_all();
    unsafe {
        UnregisterHotKey(null_mut(), STOP_HOTKEY_ID);
        UnhookWindowsHookEx(keyboard);
        UnhookWindowsHookEx(mouse);
    }
    HOOK_CONTEXT.with(|context| *context.borrow_mut() = None);
}

fn win32_error(context: &str) -> String {
    let code = unsafe { GetLastError() };
    if code == 0 {
        format!("{context}; Windows did not report an error code")
    } else {
        format!(
            "{context} (Windows error {code}: {})",
            std::io::Error::from_raw_os_error(code as i32)
        )
    }
}

trait Backend: Send + 'static {
    fn foreground_allowed(&self) -> bool;
    fn send_mouse(&self, flags: u32) -> Result<(), String>;
}

struct WindowsBackend;

impl Backend for WindowsBackend {
    fn foreground_allowed(&self) -> bool {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.is_null() {
            return false;
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(foreground, &mut process_id) };
        thread_id != 0 && process_id != unsafe { GetCurrentProcessId() }
    }

    fn send_mouse(&self, flags: u32) -> Result<(), String> {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: flags,
                    dwExtraInfo: INPUT_MARKER,
                    ..Default::default()
                },
            },
        };
        unsafe { SetLastError(0) };
        if unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) } == 1 {
            Ok(())
        } else {
            Err(win32_error(
                "Windows rejected mouse input. The target may require the same permission level",
            ))
        }
    }
}

fn eligible(state: &State) -> bool {
    state.enabled
        && state.physical_left_down
        && state.foreground_allowed
        && !state.capturing_key
        && !state.stopping
}

fn period(cps: u32) -> Duration {
    Duration::from_secs_f64(1.0 / cps.clamp(1, 30) as f64)
}

fn release_pulse(cps: u32) -> Duration {
    // Cinnabar samples button state once per frame. A fixed 5 ms up can fit
    // entirely between frames, so leave an observable release at every rate.
    (period(cps) / 2).min(Duration::from_millis(40))
}

fn next_deadline(previous: Instant, now: Instant, interval: Duration) -> Instant {
    let scheduled = previous + interval;
    if scheduled <= now {
        now + interval
    } else {
        scheduled
    }
}

/// The native backend can synchronously call the low-level hook thread. Never
/// invoke it while holding the state mutex that physical hook callbacks need.
fn emit<'a, B: Backend>(
    backend: &B,
    shared: &'a Shared,
    flags: u32,
) -> (MutexGuard<'a, State>, bool) {
    let result = backend.send_mouse(flags);
    let mut state = shared.lock();
    let accepted = match result {
        Ok(()) => true,
        Err(error) => {
            state.input_error = Some(error);
            state.enabled = false;
            state.clicking = false;
            false
        }
    };
    (state, accepted)
}

/// Foreground queries are also kept outside the hook/UI state lock.
fn foreground_state<'a, B: Backend>(backend: &B, shared: &'a Shared) -> MutexGuard<'a, State> {
    let allowed = backend.foreground_allowed();
    let mut state = shared.lock();
    state.foreground_allowed = allowed;
    state
}

fn worker_main<B: Backend>(shared: Arc<Shared>, backend: B) {
    let mut state = shared.lock();
    let mut generated_down = false;
    let mut next = Instant::now();
    let mut seen_revision = state.revision;
    loop {
        drop(state);
        state = foreground_state(&backend, &shared);
        if state.revision != seen_revision {
            seen_revision = state.revision;
            // A live CPS edit must not turn the next cycle into a second immediate click.
            next = if generated_down {
                Instant::now() + period(state.cps)
            } else {
                Instant::now()
            };
        }
        if !eligible(&state) {
            state.clicking = false;
            if generated_down {
                drop(state);
                let (updated, accepted) = emit(&backend, &shared, MOUSEEVENTF_LEFTUP);
                state = updated;
                if accepted {
                    generated_down = false;
                }
            }
            if state.stopping {
                break;
            }
            next = Instant::now();
            state = shared
                .wake
                .wait_timeout(state, POLL_INTERVAL)
                .unwrap_or_else(|error| error.into_inner())
                .0;
            continue;
        }
        state.clicking = true;
        let now = Instant::now();
        if now < next {
            state = shared
                .wake
                .wait_timeout(state, (next - now).min(POLL_INTERVAL))
                .unwrap_or_else(|error| error.into_inner())
                .0;
            continue;
        }
        drop(state);
        let (updated, accepted) = emit(&backend, &shared, MOUSEEVENTF_LEFTUP);
        state = updated;
        if !accepted {
            continue;
        }
        generated_down = false;
        let pulse_end = Instant::now() + release_pulse(state.cps);
        loop {
            drop(state);
            state = foreground_state(&backend, &shared);
            let now = Instant::now();
            if !eligible(&state) || now >= pulse_end {
                break;
            }
            state = shared
                .wake
                .wait_timeout(state, (pulse_end - now).min(POLL_INTERVAL))
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
        if eligible(&state) {
            drop(state);
            let (updated, accepted) = emit(&backend, &shared, MOUSEEVENTF_LEFTDOWN);
            state = updated;
            if accepted {
                generated_down = true;
                state.clicks = state.clicks.saturating_add(1);
            }
            // A physical release, F10, binding capture, or focus change can occur
            // while SendInput is in flight. Recheck the latest state and release
            // an accepted down immediately instead of waiting for the next cycle.
            drop(state);
            state = foreground_state(&backend, &shared);
            if generated_down && !eligible(&state) {
                state.clicking = false;
                drop(state);
                let (updated, accepted) = emit(&backend, &shared, MOUSEEVENTF_LEFTUP);
                state = updated;
                if accepted {
                    generated_down = false;
                }
            }
        }
        seen_revision = state.revision;
        next = next_deadline(next, Instant::now(), period(state.cps));
    }
    state.clicking = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    #[cfg(target_os = "windows")]
    #[ignore = "requires an interactive Windows desktop and free F10; installs mouse/keyboard hooks"]
    fn native_startup_and_shutdown_leave_mouse_input_disabled() {
        let engine = Engine::new().expect("native engine startup failed; ensure F10 is free");
        let snapshot = engine.snapshot();
        assert!(!snapshot.enabled);
        assert!(!snapshot.clicking);
        assert_eq!(snapshot.clicks, 0);
        assert!(snapshot.hotkeys_available);
        assert_eq!(snapshot.active_toggle_key, Some(ToggleKey::F8));
        drop(engine);

        // A second successful startup verifies that shutdown released F10 and both hooks.
        // Neither engine is enabled, so this test never generates desktop mouse input.
        let restarted =
            Engine::new().expect("shutdown did not release the native hook/hotkey resources");
        let snapshot = restarted.snapshot();
        assert!(!snapshot.enabled);
        assert!(!snapshot.clicking);
        assert_eq!(snapshot.clicks, 0);
        assert!(snapshot.hotkeys_available);
        drop(restarted);
    }

    #[derive(Clone)]
    struct MockBackend {
        allowed: Arc<AtomicBool>,
        reject: Arc<AtomicBool>,
        events: Arc<Mutex<Vec<u32>>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                allowed: Arc::new(AtomicBool::new(true)),
                reject: Arc::new(AtomicBool::new(false)),
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn count(&self) -> usize {
            self.events.lock().unwrap().len()
        }
    }

    impl Backend for MockBackend {
        fn foreground_allowed(&self) -> bool {
            self.allowed.load(Ordering::SeqCst)
        }
        fn send_mouse(&self, flags: u32) -> Result<(), String> {
            if self.reject.load(Ordering::SeqCst) {
                return Err("mock input rejected".into());
            }
            self.events.lock().unwrap().push(flags);
            Ok(())
        }
    }

    fn until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !predicate() {
            assert!(
                Instant::now() < deadline,
                "worker did not reach the expected state"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn mocked_engine<B: Backend>(backend: B) -> Engine {
        let shared = Shared::new();
        mocked_engine_with_shared(shared, backend)
    }

    fn mocked_engine_with_shared<B: Backend>(shared: Arc<Shared>, backend: B) -> Engine {
        let worker_shared = shared.clone();
        Engine {
            inner: Arc::new(Inner {
                shared,
                hook_thread_id: 0,
                hook_thread: None,
                worker_thread: Some(thread::spawn(move || worker_main(worker_shared, backend))),
            }),
        }
    }

    #[derive(Clone, Copy)]
    enum InFlightChange {
        PhysicalRelease,
        EmergencyStop,
        BindingCapture,
        ForegroundLoss,
    }

    #[derive(Clone)]
    struct ReentrantBackend {
        shared: Arc<Shared>,
        allowed: Arc<AtomicBool>,
        probing: Arc<AtomicBool>,
        blocked: Arc<AtomicBool>,
        fired: Arc<AtomicBool>,
        events: Arc<Mutex<Vec<u32>>>,
        trigger: u32,
        change: InFlightChange,
    }

    impl ReentrantBackend {
        fn new(shared: Arc<Shared>, trigger: u32, change: InFlightChange) -> Self {
            Self {
                shared,
                allowed: Arc::new(AtomicBool::new(true)),
                probing: Arc::new(AtomicBool::new(true)),
                blocked: Arc::new(AtomicBool::new(false)),
                fired: Arc::new(AtomicBool::new(false)),
                events: Arc::new(Mutex::new(Vec::new())),
                trigger,
                change,
            }
        }

        fn hook_can_reenter(&self) -> bool {
            if !self.probing.load(Ordering::SeqCst) {
                return true;
            }
            // Never block a failed regression test forever: detect the mutex
            // being held before simulating a synchronous native hook callback.
            if self.shared.state.try_lock().is_err() {
                self.blocked.store(true, Ordering::SeqCst);
                return false;
            }
            self.shared.observe_keyboard(WM_KEYDOWN, b'Z' as u32, 0);
            self.shared.observe_keyboard(WM_KEYUP, b'Z' as u32, 0);
            true
        }
    }

    impl Backend for ReentrantBackend {
        fn foreground_allowed(&self) -> bool {
            self.hook_can_reenter() && self.allowed.load(Ordering::SeqCst)
        }

        fn send_mouse(&self, flags: u32) -> Result<(), String> {
            if !self.hook_can_reenter() {
                return Err("native hook would block on the worker state mutex".into());
            }
            self.events.lock().unwrap().push(flags);
            self.shared.observe_mouse(
                if flags == MOUSEEVENTF_LEFTDOWN {
                    WM_LBUTTONDOWN
                } else {
                    WM_LBUTTONUP
                },
                LLMHF_INJECTED,
            );
            if flags == self.trigger && !self.fired.swap(true, Ordering::SeqCst) {
                match self.change {
                    InFlightChange::PhysicalRelease => {
                        self.shared.observe_mouse(WM_LBUTTONUP, 0);
                    }
                    InFlightChange::EmergencyStop => {
                        self.shared.observe_keyboard(WM_KEYDOWN, VK_F10 as u32, 0);
                    }
                    InFlightChange::BindingCapture => {
                        let mut state = self.shared.lock();
                        state.enabled = false;
                        state.capturing_key = true;
                        state.revision = state.revision.wrapping_add(1);
                        drop(state);
                        self.shared.wake.notify_all();
                    }
                    InFlightChange::ForegroundLoss => {
                        self.allowed.store(false, Ordering::SeqCst);
                    }
                }
            }
            Ok(())
        }
    }

    #[test]
    fn native_backend_reentry_and_in_flight_changes_cannot_latch_a_down() {
        for change in [
            InFlightChange::PhysicalRelease,
            InFlightChange::EmergencyStop,
            InFlightChange::BindingCapture,
            InFlightChange::ForegroundLoss,
        ] {
            for trigger in [MOUSEEVENTF_LEFTUP, MOUSEEVENTF_LEFTDOWN] {
                let shared = Shared::new();
                // Configure before starting the worker, so the probe specifically
                // detects a lock held by the worker during its backend call.
                {
                    let mut state = shared.lock();
                    state.enabled = true;
                    state.physical_left_down = true;
                    state.cps = 30;
                }
                let backend = ReentrantBackend::new(shared.clone(), trigger, change);
                let engine = mocked_engine_with_shared(shared.clone(), backend.clone());
                let expected = if trigger == MOUSEEVENTF_LEFTDOWN {
                    3
                } else {
                    1
                };
                until(|| {
                    backend.blocked.load(Ordering::SeqCst)
                        || (backend.fired.load(Ordering::SeqCst)
                            && backend.events.lock().unwrap().len() >= expected)
                });
                // The condition above uses only backend atomics/events. Disable
                // probing before main-thread shutdown legitimately takes the lock.
                backend.probing.store(false, Ordering::SeqCst);
                // Observe two full click periods before shutdown. Dropping the
                // engine immediately could hide a stale-down or extra-cycle bug.
                thread::sleep(period(30) * 2);
                let before_shutdown = backend.events.lock().unwrap().clone();
                drop(engine);
                assert!(
                    !backend.blocked.load(Ordering::SeqCst),
                    "backend/native hook was called with the worker state locked"
                );
                let events = backend.events.lock().unwrap();
                if trigger == MOUSEEVENTF_LEFTDOWN {
                    assert_eq!(
                        before_shutdown.as_slice(),
                        [MOUSEEVENTF_LEFTUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP]
                    );
                    assert_eq!(
                        events.as_slice(),
                        [MOUSEEVENTF_LEFTUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP]
                    );
                } else {
                    assert_eq!(before_shutdown.as_slice(), [MOUSEEVENTF_LEFTUP]);
                    assert_eq!(events.as_slice(), [MOUSEEVENTF_LEFTUP]);
                }
                assert!(!shared.lock().clicking);
            }
        }
    }

    #[test]
    fn injected_input_does_not_latch_or_release_the_physical_hold() {
        let shared = Shared::new();
        shared.observe_mouse(WM_LBUTTONDOWN, LLMHF_INJECTED);
        shared.observe_mouse(WM_LBUTTONDOWN, LLMHF_LOWER_IL_INJECTED);
        assert!(!shared.lock().physical_left_down);
        shared.observe_mouse(WM_LBUTTONDOWN, 0);
        shared.observe_mouse(WM_LBUTTONUP, LLMHF_INJECTED);
        assert!(shared.lock().physical_left_down);
        shared.observe_mouse(WM_LBUTTONUP, 0);
        assert!(!shared.lock().physical_left_down);
    }

    fn keyboard_engine() -> Engine {
        let engine = Engine::new_preview();
        let mut state = engine.inner.shared.lock();
        state.active_toggle_key = Some(state.toggle_key);
        drop(state);
        engine
    }

    #[test]
    fn captured_key_is_consumed_until_release_then_toggles_once_per_press() {
        let engine = keyboard_engine();
        let shared = &engine.inner.shared;
        engine.set_enabled(true);
        engine.begin_key_capture();
        assert!(!engine.snapshot().enabled);
        engine.set_enabled(true);
        engine.toggle();
        assert!(!engine.snapshot().enabled);
        let previous_revision = engine.snapshot().binding_revision;
        assert!(shared.observe_keyboard(WM_KEYDOWN, b'K' as u32, 0));
        assert!(!engine.snapshot().capturing_key);
        assert!(!engine.snapshot().enabled);
        assert_eq!(engine.snapshot().toggle_key.virtual_key(), b'K' as u32);
        assert_eq!(engine.snapshot().binding_revision, previous_revision + 1);
        for _ in 0..5 {
            assert!(shared.observe_keyboard(WM_KEYDOWN, b'K' as u32, 0));
            assert!(!engine.snapshot().enabled);
        }
        assert!(shared.observe_keyboard(WM_KEYUP, b'K' as u32, 0));
        assert!(shared.observe_keyboard(WM_KEYDOWN, b'K' as u32, 0));
        assert!(engine.snapshot().enabled);
        assert!(shared.observe_keyboard(WM_KEYDOWN, b'K' as u32, 0));
        assert!(engine.snapshot().enabled);
        assert!(shared.observe_keyboard(WM_KEYUP, b'K' as u32, 0));
        assert!(shared.observe_keyboard(WM_KEYDOWN, b'K' as u32, 0));
        assert!(!engine.snapshot().enabled);
    }

    #[test]
    fn selected_toggle_is_reserved_until_release_without_leaking_game_bindings() {
        for key in [0x1B, b'E' as u32, 0xA4, ToggleKey::F8.virtual_key()] {
            let engine = keyboard_engine();
            let shared = &engine.inner.shared;
            engine.set_toggle_key(ToggleKey::from_virtual_key(key).unwrap());
            assert!(!shared.observe_keyboard(WM_KEYDOWN, key, LLKHF_INJECTED));
            assert!(!engine.snapshot().enabled);
            assert!(shared.observe_keyboard(WM_SYSKEYDOWN, key, 0));
            assert!(engine.snapshot().enabled);
            for _ in 0..3 {
                assert!(shared.observe_keyboard(WM_SYSKEYDOWN, key, 0));
                assert!(engine.snapshot().enabled);
            }
            assert!(shared.observe_keyboard(WM_SYSKEYUP, key, 0));
            assert!(shared.observe_keyboard(WM_KEYDOWN, key, 0));
            assert!(!engine.snapshot().enabled);
            assert!(shared.observe_keyboard(WM_KEYUP, key, 0));
            assert!(!shared.observe_keyboard(WM_KEYDOWN, b'Z' as u32, 0));
        }
    }

    #[test]
    fn capture_can_replace_old_toggle_and_accept_modifiers_and_system_keys() {
        let engine = keyboard_engine();
        let shared = &engine.inner.shared;
        engine.begin_key_capture();
        assert!(shared.observe_keyboard(WM_KEYDOWN, ToggleKey::F8.virtual_key(), 0));
        assert!(!engine.snapshot().enabled);
        assert!(!engine.snapshot().capturing_key);
        shared.observe_keyboard(WM_KEYUP, ToggleKey::F8.virtual_key(), 0);
        engine.begin_key_capture();
        assert!(shared.observe_keyboard(WM_SYSKEYDOWN, 0xA4, 0));
        assert_eq!(engine.snapshot().toggle_key.label(), "Left Alt");
        assert!(shared.observe_keyboard(WM_SYSKEYUP, 0xA4, 0));
        shared.observe_keyboard(WM_SYSKEYDOWN, 0xA4, 0);
        assert!(engine.snapshot().enabled);
    }

    #[test]
    fn capture_ignores_injected_keys_and_existing_holds_and_reserves_f10() {
        let engine = keyboard_engine();
        let shared = &engine.inner.shared;
        // A key already held when capture starts cannot be chosen by autorepeat.
        shared.observe_keyboard(WM_KEYDOWN, b'A' as u32, 0);
        engine.begin_key_capture();
        assert!(!shared.observe_keyboard(WM_KEYDOWN, b'B' as u32, LLKHF_INJECTED));
        assert!(!shared.observe_keyboard(WM_KEYDOWN, b'C' as u32, LLKHF_LOWER_IL_INJECTED));
        assert!(!shared.observe_keyboard(WM_KEYDOWN, b'A' as u32, 0));
        assert!(engine.snapshot().capturing_key);
        assert!(shared.observe_keyboard(WM_KEYDOWN, VK_F10 as u32, 0));
        assert!(engine.snapshot().capturing_key);
        assert_eq!(engine.snapshot().toggle_key, ToggleKey::F8);
        assert!(!engine.snapshot().enabled);
        assert!(engine.snapshot().hotkey_error.unwrap().contains("reserved"));
        assert!(shared.observe_keyboard(WM_KEYUP, VK_F10 as u32, 0));
        shared.observe_keyboard(WM_KEYUP, b'A' as u32, 0);
        assert!(shared.observe_keyboard(WM_KEYDOWN, b'A' as u32, 0));
        assert_eq!(engine.snapshot().toggle_key.virtual_key(), b'A' as u32);
        assert!(engine.snapshot().hotkey_error.is_none());
    }

    #[test]
    fn emergency_stop_notifies_other_features_when_clicker_is_off() {
        let engine = keyboard_engine();
        assert!(!engine.snapshot().enabled);
        let initial = engine.snapshot().stop_revision;
        engine
            .inner
            .shared
            .observe_keyboard(WM_KEYDOWN, VK_F10 as u32, 0);
        assert_eq!(engine.snapshot().stop_revision, initial.wrapping_add(1));
        engine
            .inner
            .shared
            .observe_keyboard(WM_KEYUP, VK_F10 as u32, 0);
        engine.stop();
        assert_eq!(engine.snapshot().stop_revision, initial.wrapping_add(2));
    }

    #[test]
    fn enabling_clicker_by_ui_or_key_preserves_active_aim() {
        let engine = keyboard_engine();
        engine.inner.shared.lock().stop_registered = true;
        let mut aim = crate::aimassist::Activation::new(engine.snapshot().stop_revision);
        aim.set_enabled(true, engine.snapshot().stop_revision);
        assert!(!engine.snapshot().enabled);

        // The UI switch uses Engine::toggle, while the bound key uses the hook.
        engine.toggle();
        let snapshot = engine.snapshot();
        assert!(snapshot.enabled);
        assert_eq!(
            aim.poll_stop(snapshot.stop_revision, snapshot.hotkeys_available),
            None
        );
        assert!(aim.enabled);
        engine.toggle();
        engine
            .inner
            .shared
            .observe_keyboard(WM_KEYDOWN, ToggleKey::F8.virtual_key(), 0);
        let snapshot = engine.snapshot();
        assert!(snapshot.enabled);
        assert_eq!(
            aim.poll_stop(snapshot.stop_revision, snapshot.hotkeys_available),
            None
        );
        assert!(aim.enabled);

        engine
            .inner
            .shared
            .observe_keyboard(WM_KEYUP, ToggleKey::F8.virtual_key(), 0);
        engine
            .inner
            .shared
            .observe_keyboard(WM_KEYDOWN, VK_F10 as u32, 0);
        let snapshot = engine.snapshot();
        assert!(!snapshot.enabled);
        assert_eq!(
            aim.poll_stop(snapshot.stop_revision, snapshot.hotkeys_available),
            Some("Stopped by F10.")
        );
        assert!(!aim.enabled);
    }

    #[test]
    fn physical_stop_is_consumed_once_including_repeat_and_release() {
        let engine = keyboard_engine();
        let shared = &engine.inner.shared;
        assert!(shared.observe_keyboard(WM_KEYDOWN, VK_F10 as u32, 0));
        let revision = engine.snapshot().stop_revision;
        assert!(shared.observe_keyboard(WM_KEYDOWN, VK_F10 as u32, 0));
        assert!(shared.observe_keyboard(WM_KEYUP, VK_F10 as u32, 0));
        assert_eq!(engine.snapshot().stop_revision, revision);
        assert!(shared.observe_keyboard(WM_KEYDOWN, VK_F10 as u32, 0));
        assert_eq!(engine.snapshot().stop_revision, revision.wrapping_add(1));
    }

    #[test]
    fn cancelled_capture_preserves_binding_and_f10_stops_with_modifiers() {
        let engine = keyboard_engine();
        engine.set_enabled(true);
        engine.begin_key_capture();
        engine.cancel_key_capture();
        assert!(!engine.snapshot().capturing_key);
        assert!(!engine.snapshot().enabled);
        assert_eq!(engine.snapshot().toggle_key, ToggleKey::F8);
        engine.set_enabled(true);
        engine
            .inner
            .shared
            .observe_keyboard(WM_SYSKEYDOWN, VK_F10 as u32, 0);
        assert!(!engine.snapshot().enabled);
        engine.toggle();
        assert!(engine.snapshot().enabled);
        // The keyboard hook ignores synthetic events. Registered F10 remains an
        // independent emergency-stop fallback through the native WM_HOTKEY path.
        engine
            .inner
            .shared
            .observe_keyboard(WM_SYSKEYDOWN, VK_F10 as u32, LLKHF_INJECTED);
        assert!(engine.snapshot().enabled);
    }

    #[test]
    fn arbitrary_bindings_round_trip_without_keyboard_layout_dependency() {
        for value in [
            3, 8, 13, 32, 37, 48, 65, 90, 96, 112, 123, 135, 160, 165, 186, 222, 254,
        ] {
            let key = ToggleKey::from_virtual_key(value).unwrap();
            assert_eq!(ToggleKey::from_label(&key.storage_label()), Some(key));
            assert!(!key.label().is_empty());
        }
        for value in [0, 1, 2, 4, 5, 6, VK_F10 as u32, 255, u32::MAX] {
            assert!(ToggleKey::from_virtual_key(value).is_none());
        }
        assert_eq!(
            ToggleKey::from_label("f12"),
            ToggleKey::from_virtual_key(0x7B)
        );
        assert_eq!(ToggleKey::from_label("f24").unwrap().virtual_key(), 0x87);
        assert_eq!(ToggleKey::from_label("space").unwrap().virtual_key(), 32);
        assert_eq!(ToggleKey::from_label("k").unwrap().virtual_key(), 75);
        assert!(ToggleKey::from_label("F10").is_none());
        assert!(ToggleKey::from_label("VK:121").is_none());
        assert!(ToggleKey::from_label("VK:999999").is_none());
    }

    #[test]
    fn beginning_capture_releases_mock_clicking_and_cancellation_stays_disabled() {
        let backend = MockBackend::new();
        let engine = mocked_engine(backend.clone());
        engine.inner.shared.observe_mouse(WM_LBUTTONDOWN, 0);
        engine.set_enabled(true);
        until(|| engine.snapshot().clicks > 0);
        engine.begin_key_capture();
        until(|| !engine.snapshot().clicking);
        assert_eq!(
            backend.events.lock().unwrap().last(),
            Some(&MOUSEEVENTF_LEFTUP)
        );
        let stopped_count = backend.count();
        engine.cancel_key_capture();
        thread::sleep(Duration::from_millis(45));
        assert_eq!(backend.count(), stopped_count);
        assert!(!engine.snapshot().enabled);
    }

    #[test]
    fn steady_schedule_skips_stalls_without_a_catch_up_burst() {
        let start = Instant::now();
        let interval = period(10);
        assert_eq!(
            next_deadline(start, start + Duration::from_millis(5), interval),
            start + interval
        );
        let stalled = start + Duration::from_secs(2);
        assert_eq!(next_deadline(start, stalled, interval), stalled + interval);
        assert_eq!(period(0), Duration::from_secs(1));
        assert_eq!(period(100), period(30));
    }

    #[test]
    fn release_pulse_remains_visible_and_leaves_a_down_phase_at_every_rate() {
        assert_eq!(release_pulse(12), Duration::from_millis(40));
        assert_eq!(release_pulse(30), period(30) / 2);
        for cps in 1..=30 {
            let release = release_pulse(cps);
            assert!(release > Duration::ZERO);
            assert!(release <= Duration::from_millis(40));
            assert!(release < period(cps));
        }
    }

    #[test]
    fn release_stop_foreground_and_shutdown_release_generated_down() {
        let backend = MockBackend::new();
        let engine = mocked_engine(backend.clone());
        engine.set_cps(30);
        engine.set_enabled(true);
        engine
            .inner
            .shared
            .observe_mouse(WM_LBUTTONDOWN, LLMHF_INJECTED);
        thread::sleep(Duration::from_millis(40));
        assert_eq!(backend.count(), 0);
        engine.inner.shared.observe_mouse(WM_LBUTTONDOWN, 0);
        until(|| engine.snapshot().clicks >= 3);
        {
            let events = backend.events.lock().unwrap();
            assert_eq!(
                &events[..6],
                &[
                    MOUSEEVENTF_LEFTUP,
                    MOUSEEVENTF_LEFTDOWN,
                    MOUSEEVENTF_LEFTUP,
                    MOUSEEVENTF_LEFTDOWN,
                    MOUSEEVENTF_LEFTUP,
                    MOUSEEVENTF_LEFTDOWN
                ]
            );
        }
        backend.allowed.store(false, Ordering::SeqCst);
        until(|| !engine.snapshot().clicking);
        let paused = backend.count();
        assert_eq!(
            backend.events.lock().unwrap().last(),
            Some(&MOUSEEVENTF_LEFTUP)
        );
        thread::sleep(Duration::from_millis(45));
        assert_eq!(backend.count(), paused);
        backend.allowed.store(true, Ordering::SeqCst);
        until(|| engine.snapshot().clicks >= 4);
        engine.inner.shared.observe_mouse(WM_LBUTTONUP, 0);
        until(|| !engine.snapshot().clicking);
        let released = backend.count();
        thread::sleep(Duration::from_millis(45));
        assert_eq!(backend.count(), released);
        engine.stop();
        engine.inner.shared.observe_mouse(WM_LBUTTONDOWN, 0);
        thread::sleep(Duration::from_millis(45));
        assert_eq!(backend.count(), released);
        engine.toggle();
        until(|| engine.snapshot().clicks >= 5);
        drop(engine);
        assert_eq!(
            backend.events.lock().unwrap().last(),
            Some(&MOUSEEVENTF_LEFTUP)
        );
    }

    #[test]
    fn input_failure_disarms_and_can_be_retried_explicitly() {
        let backend = MockBackend::new();
        let engine = mocked_engine(backend.clone());
        backend.reject.store(true, Ordering::SeqCst);
        engine.inner.shared.observe_mouse(WM_LBUTTONDOWN, 0);
        engine.set_enabled(true);
        until(|| engine.snapshot().input_error.is_some());
        assert!(!engine.snapshot().enabled);
        assert_eq!(engine.snapshot().clicks, 0);
        backend.reject.store(false, Ordering::SeqCst);
        engine.set_enabled(true);
        until(|| engine.snapshot().clicks > 0);
        assert!(engine.snapshot().input_error.is_none());
    }

    #[test]
    fn preview_has_no_desktop_threads_and_controls_clamp_cps() {
        let preview = Engine::new_preview();
        preview.set_cps(0);
        assert_eq!(preview.snapshot().cps, 1);
        preview.set_cps(100);
        assert_eq!(preview.snapshot().cps, 30);
        preview.toggle();
        assert!(preview.snapshot().enabled);
        assert!(!preview.snapshot().clicking);
        assert!(!preview.snapshot().hotkeys_available);
        let f12 = ToggleKey::from_virtual_key(0x7B).unwrap();
        preview.set_toggle_key(f12);
        assert_eq!(preview.snapshot().toggle_key, f12);
        assert_eq!(ToggleKey::from_label("f8"), Some(ToggleKey::F8));
        assert_eq!(ToggleKey::from_label("F10"), None);
    }
}

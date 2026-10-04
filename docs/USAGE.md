# Cinnabar clicker

Standalone Rust + GPUI clicker for Windows x64. Version **1.4.0** uses a compact 680 × 480 window with Clicker and Preferences tabs. Dark mode defaults to neutral charcoal; light mode uses white surfaces. The interface has no sidebar, decorative glows, or accent selector. Top-right controls minimize, maximize or restore, and close the window.

## Run

Close the older clicker, then open **CinnabarClicker.exe**.

- **F8** enables or disables the clicker by default.
- Hold the physical left mouse button in another foreground application to click. Release to pause.
- **F10** disables globally and remains reserved for emergency stop.
- Set speed from **1–30 CPS** with the slider or plus/minus buttons.
- In Preferences, click **Change**, then press any keyboard key to bind it. Release the captured key and press it again to toggle. Holding it toggles only once.
- Preferences also controls dark mode and minimize on enable. Settings save automatically.
- Drag the blank header or app-name area to move the window. Resize from its edges.

The app starts disabled, works system wide, and pauses over its own controls. Minimizing keeps the input engine and hotkeys running. Closing stops both input threads and releases generated mouse-down input.

The chosen toggle key is consumed while the app is open, including repeats and key-up. Use a key you do not need in other applications. Letters, numbers, function keys, arrows, punctuation, and modifiers can be bound; F10 is reserved. Click Cancel to leave key capture. Switching tabs or leaving the window also cancels. Capture disables clicking until you enable it again.

F10 must be available before clicking can start. If another app owns it, close that app and reopen Cinnabar clicker. Windows may reject generated input into an application running at a higher permission level; run both at the same level. Some applications ignore generated mouse input. Live Cinnabar gameplay acceptance has not been verified.

Preferences save to `%LOCALAPPDATA%\CinnabarClicker\settings.json`. Existing CPS, toggle-key, minimize, and dark/light settings are preserved; the removed accent preference is ignored.

## Build

Requirements: Windows x64, Rust stable supporting edition 2024 (tested with 1.93.1), and Visual Studio C++ Build Tools with a Windows SDK. In this directory:

```powershell
cargo build --release --locked
cargo test --release --locked
cargo clippy --release --all-targets --locked -- -D warnings
```

The binary is `target\release\cinnabar-clicker.exe`; rename it to `CinnabarClicker.exe` for distribution. The icon and assets are embedded. GPUI is pinned to 0.2.2, with exact transitive versions in Cargo.lock. The local Cargo configuration links the MSVC runtime statically. GPUI still requires Windows graphics libraries and a DirectX-capable driver.

## Validation and previews

Engine tests use a mock input backend and generate no desktop clicks. They cover physical/injected input tracking, scheduling, speed limits, release and shutdown cleanup, input failure recovery, arbitrary keys, capture, cancellation, held-key repetition, modifiers, F10 reservation, reentrant hooks, and toggle-key consumption. Settings tests cover migration and persistence, including default-on dark mode and explicit light mode.

The ignored `native_startup_and_shutdown_leave_mouse_input_disabled` test starts and stops the real hooks and F10 hotkey while clicking stays disabled. It requires free F10:

```powershell
cargo test --release native_startup_and_shutdown_leave_mouse_input_disabled -- --ignored
```

`CinnabarClicker.exe --smoke-test` opens the actual interface briefly and exits using a preview engine with no hooks, hotkeys, input injection, or settings writes. Add `--page=settings` to preview Preferences or `--light` to preview light mode. These options apply only to smoke mode. Native window checks use only the launched smoke window and generate no desktop input. See VALIDATION.txt for recorded results.

## Implementation

Mouse and keyboard hooks and the registered F10 stop hotkey run on a continuously pumped thread. The keyboard hook supports arbitrary toggle keys without RegisterHotKey restrictions. Keys are stored as Windows virtual-key codes; legacy function-key settings still load.

A separate worker schedules click pulses with a monotonic clock. It performs Windows calls outside the shared input-state mutex so physical input callbacks can continue processing. State is checked again after delivery to clean up a generated down if the user releases, disables, changes focus, or captures a key. Injected input does not count as a physical hold, and delayed ticks never trigger catch-up bursts. The release interval is half the click period, capped at 40 ms. The app does not inject into game processes.

The outer surface fills the native client area. Windows controls the corners and outline; there is no second rounded frame. The native-frame helper suppresses the extra border and removes the restored-window top inset while preserving GPUI resize and maximized-window handling. Corners can be rectangular when maximized, snapped, or native rounding is unavailable. Windows transparency settings and graphics support determine the backdrop effect; the neutral palette remains rendered without acrylic.

Visual reference: [Cinnabar repository](https://github.com/bedrock-mc/cinnabar/) and the supplied screenshot. The mark and icons are independently drawn; provenance is in `assets/PROVENANCE.txt`. This is a separate project from the Cinnabar Minecraft client.

Application source and original assets use the MIT license in LICENSE.txt. GPUI is developed by Zed Industries under Apache-2.0. Dependency notices are included in THIRD_PARTY_NOTICES.txt.

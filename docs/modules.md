# In-game modules

Open **Cinnaroids.exe** before or while Cinnabar is running. It automatically registers the embedded module for the standard installation at `%LOCALAPPDATA%/Programs/Cinnabar/bedrock-client.exe` and shows **Attached** after the game acknowledges loading. It never starts the game or selects another installation.

The host must include live local-module loading, compact personal-panel controls, and the packet-timing capability ([host update](https://github.com/bedrock-mc/cinnabar/pull/171)). Older installations need one update and restart; subsequent attachments work while the game is running. The launcher does not change a running process's memory or load a DLL into it.

Press **Right Shift** to open or close the module interface. It manages:

- **Clicker:** hold the physical Attack control to click at the selected CPS. The toggle key is configurable; F8 is the default.
- **Aim assist:** strength from 0–100%, with **Continuous** or **While clicking** activation. Its default toggle key is R.
- **Reach:** adjustable local attack distance, bounded at six blocks. Its default toggle key is V.
- **FakeLag:** delay inbound and outbound game packets by 0–1000 milliseconds each, with a toggle in the Combat tab. Default delay is 100 ms; the module starts off. Slider values can also be typed.
- **Appearance:** neutral dark mode by default, with a light option.

Slider values can also be typed: click the current number, enter a value, and press **Enter**. **Escape** cancels the edit. The host applies the slider's range and step. **Mode** opens a dropdown with explicit options instead of cycling when clicked. Click outside or press Escape to dismiss it. Editing consumes typing so module shortcuts do not toggle accidentally; **F10** and **Right Shift** retain their reserved actions.

To change a module's binding, press its **Keybind** keycap, then press the next keyboard key. Each module has its own binding; a key already assigned to another module cannot be reused. **Right Shift** and **F10** are reserved for the panel and emergency stop. **Escape** can be assigned during key capture; otherwise it closes the panel. Closing the panel or leaving the game window cancels key capture.

**F10** stops all modules. Enabled switches are runtime state and start off for a new session. Closing the launcher disables its registration and unloads the modules while leaving Cinnabar running. They stay unloaded after a game restart until the launcher is opened again; saved preferences are preserved. Opening the module interface releases gameplay input so its controls can be used normally.

Aim assist reads player coordinates, selects a target within six blocks and a 30-degree cone, and smoothly steers toward the chest. Strength zero produces no motion. The API exposes positions without visibility or team information, so the module cannot filter occluded players or teammates. Menus, loss of cursor capture, and server-controlled cameras pause assistance.

Reach changes the client's actor attack selection distance. It preserves block targeting and normal server authority; a server can reject an attack outside its own range.

FakeLag delays application packets in both directions while preserving their order; the selected delay adds to each direction separately. Login and transport acknowledgments continue normally. Turning it off or pressing F10 releases queued packets; closing the launcher removes the delay. A host heartbeat expires the delay if the client stops responding. **Show real position** draws a box at your own last movement position forwarded upstream, before newer movement leaves the delay queue. This is a sent-position witness, not a server acknowledgment; server corrections or rejected movement may differ. The box clears when FakeLag or this option is off, on disconnect, or when its position feed becomes stale.

## Runtime and settings

The launcher installs `%LOCALAPPDATA%/Cinnaroids/mods/cinnaroids.component.wasm`. The host stores module preferences in its fixed companion, `cinnaroids.component.settings.json`. Valid preferences from the older `%LOCALAPPDATA%/CinnabarClicker/settings.json` migrate only if the companion is absent. Existing module preferences and the old file are preserved; runtime enabled switches are not migrated.

The launcher atomically writes `%LOCALAPPDATA%/Cinnabar/local-mod.json` with the component, font and explicit player, camera, controls, interaction, packet timing and settings grants. Matching launcher instances reuse an unchanged registration. Replaced assets, a failed live request, or a conflicting loaded acknowledgment require a fresh request. Attachment failures retry after 2, 4, and 8 seconds; a game restart or installed-client update resets that retry budget. Errors remain visible until recovery. The running host polls and compiles on a worker and reports the request ID, client PID and loading outcome in `local-mod.status.json`. The launcher verifies that acknowledgment belongs to the standard installed running client. The WASM component has no WASI or ambient operating-system access.

The panel uses Cinnabar's installed Cinnangles Sans font. Cinnaroids registers no private font override. Panel sizing follows display DPI independently of Minecraft's GUI scale.

The launcher uses the installed Cinnabar's resources without overriding its asset path. Module preferences, client settings and caches remain in place. Client binaries and Minecraft assets are excluded from the public repository.

## Build

A source checkout needs Cinnabar's extended personal-mod API and live registration loader, built with `--features local-mods`, plus the matching Go binaries and resources in the standard installation folder.

Rebuild the guest with `powershell -ExecutionPolicy Bypass -File scripts/build-cinnaroids.ps1`, then build the launcher with `cargo build --release --locked`. Guest tests use `cargo test --manifest-path mods/cinnaroids/Cargo.toml --locked`.

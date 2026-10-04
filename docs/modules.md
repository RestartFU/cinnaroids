# In-game modules

Open **Cinnaroids.exe** with your installed Cinnabar already running. It registers the embedded module automatically and shows **Attached** after the game acknowledges loading. **Start Cinnabar** starts the installed client only when no matching process is running.

The host must include live local-module loading. Older installations need one update and restart; subsequent attachments work while the game is running. The launcher does not change a running process's memory or load a DLL into it.

Press **Right Shift** to open or close the module interface. It manages:

- **Clicker:** hold the physical Attack control to click at the selected CPS. The toggle key is configurable; F8 is the default.
- **Aim assist:** strength from 0–100%, with **Continuous** or **While clicking** activation.
- **Reach:** adjustable local attack distance, bounded at six blocks.
- **Appearance:** neutral dark mode by default, with a light option.

To change the clicker binding, press **Toggle key**, then press the next keyboard key. **Right Shift** and **F10** are reserved for the panel and emergency stop. **Escape** can be assigned during key capture; otherwise it closes the panel. Closing the panel or leaving the game window cancels key capture.

**F10** stops all modules. Enabled switches are runtime state and start off for a new session. Closing the launcher leaves the running client and modules alone. Opening the module interface releases gameplay input so its controls can be used normally.

Aim assist reads player coordinates, selects a target within six blocks and a 30-degree cone, and smoothly steers toward the chest. Strength zero produces no motion. The API exposes positions without visibility or team information, so the module cannot filter occluded players or teammates. Menus, loss of cursor capture, and server-controlled cameras pause assistance.

Reach changes the client's actor attack selection distance. It preserves block targeting and normal server authority; a server can reject an attack outside its own range.

## Runtime and settings

The launcher installs `%LOCALAPPDATA%/Cinnaroids/mods/cinnaroids.component.wasm`. The host stores module preferences in its fixed companion, `cinnaroids.component.settings.json`. Valid preferences from the older `%LOCALAPPDATA%/CinnabarClicker/settings.json` migrate only if the companion is absent. Existing module preferences and the old file are preserved; runtime enabled switches are not migrated.

When the launcher starts Cinnabar, output is in `%LOCALAPPDATA%/Cinnaroids/logs/cinnabar.log`. It atomically writes `%LOCALAPPDATA%/Cinnabar/local-mod.json` with the component, font and explicit player, camera, controls, interaction and settings grants. The running host polls and compiles on a worker and reports the request ID, client PID and loading outcome in `local-mod.status.json`. The launcher verifies that acknowledgment belongs to the selected running client. The WASM component has no WASI or ambient operating-system access.

The panel uses the embedded OFL Inter Medium font, installed with its license in `%LOCALAPPDATA%/Cinnaroids/fonts/`. Registration selects it for the personal panel. The worker rasterizes it before attachment; the host updates its isolated font page without replacing game or server glyphs. Panel sizing follows display DPI independently of Minecraft's GUI scale.

The launcher uses the installed Cinnabar's resources without overriding its asset path. Module preferences, client settings and caches remain in place. Client binaries and Minecraft assets are excluded from the public repository.

## Build

A source checkout needs Cinnabar's extended personal-mod API and live registration loader, built with `--features local-mods`, plus the matching Go binaries and installed resources. Select its installed `bedrock-client.exe` with **Choose client** if it is outside the standard installation folder.

Rebuild the guest with `powershell -ExecutionPolicy Bypass -File scripts/build-cinnaroids.ps1`, then build the launcher with `cargo build --release --locked`. Guest tests use `cargo test --manifest-path mods/cinnaroids/Cargo.toml --locked`.

# In-game modules

Open **Cinnaroids.exe** and use **Start Cinnabar**. The local package includes a compatible client beside the launcher. An existing Cinnabar session needs to be restarted through this launcher to load the module.

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

Startup output is in `%LOCALAPPDATA%/Cinnaroids/logs/cinnabar.log`. The launcher sets `CINNABAR_MOD_COMPONENT` and grants `CINNABAR_MOD_PLAYERS`, `CINNABAR_MOD_CAMERA`, `CINNABAR_MOD_CONTROLS`, `CINNABAR_MOD_INTERACTION`, and `CINNABAR_MOD_SETTINGS` for the child client. The WASM component has no WASI or ambient operating-system access.

The panel uses the bundled OFL Inter Medium font, installed with its license in `%LOCALAPPDATA%/Cinnaroids/fonts/`. `CINNABAR_MOD_FONT` selects it for the child. It is rasterized once at startup and applies only to personal-panel labels. Panel sizing follows display DPI independently of Minecraft's GUI scale.

The local package contains compatible compiled assets in `Cinnabar/assets/compiled/`. The launcher supplies their block carrier with `--assets` when present, selecting matching sidecars from the same folder. Installed Cinnabar files and caches remain in place. Client binaries and Minecraft assets are excluded from the public repository.

## Build

A source checkout needs Cinnabar's extended personal-mod API, built with `--features local-mods`, plus the matching Go binaries, resources, and compiled assets. Select its `bedrock-client.exe` with **Choose client** if it is not bundled.

Rebuild the guest with `powershell -ExecutionPolicy Bypass -File scripts/build-cinnaroids.ps1`, then build the launcher with `cargo build --release --locked`. Guest tests use `cargo test --manifest-path mods/cinnaroids/Cargo.toml --locked`.

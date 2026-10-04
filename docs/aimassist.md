# Aim assist

Open **Aim assist**, choose strength and **Continuous** or **While clicking**, then use **Start Cinnabar** to start a new compatible session. Enable aim assist after joining a world. F10 stops both aim assist and the clicker; closing the clicker disables its module. Both features start disabled.

Losing a target, releasing Attack in click-only mode, changing focus or switching tabs pauses assistance without turning its switch off. Automatic stops show their reason beside Start Cinnabar. Re-enabling acknowledges earlier stop events; a later F10 still stops both features.

The prepared local app includes a compatible client in `Cinnabar/`. An existing Cinnabar session needs to be restarted through this launcher to load the module. A source checkout requires a Cinnabar client built from commit `d392984f60372b18c99254485ad89b12cece36a5` or later with `--features local-mods`, plus its sibling Go binaries and resources. Select that `bedrock-client.exe` using Start Cinnabar if it is not bundled.

The local package also includes compatible compiled assets in `Cinnabar/assets/compiled/`. When present, the launcher passes their block asset path with `--assets`, which selects the matching entity and other carriers in the same folder. These files were prepared beside the new runtime; the installed client and its cached assets stay in place. Cinnabar binaries and Minecraft assets are excluded from the public source repository.

The launcher grants player reads and camera input to this personal WASM module. It uses player coordinates from the gameplay API, selects a target within six blocks and a 30-degree cone, and smoothly steers toward the chest. Strength zero produces no motion. The API supplies positions rather than visibility or team information, so the module cannot filter occluded players or teammates. Menus, loss of cursor capture and server-controlled cameras pause it.

Settings update the module through Cinnabar's existing reload support, normally within half a second. The module is stored at `%LOCALAPPDATA%/CinnabarClicker/mods/aimassist.component.wasm`; startup logs are in the sibling `logs/cinnabar.log`. The launcher sets `CINNABAR_MOD_COMPONENT`, `CINNABAR_MOD_PLAYERS=1` and `CINNABAR_MOD_CAMERA=1` for its child session.

The Rust guest uses the SDK pinned to the merged gameplay API. Rebuild it with `powershell -ExecutionPolicy Bypass -File scripts/build-aimassist.ps1`, then build the clicker with `cargo build --release --locked`. Guest tests: `cargo test --manifest-path mods/aimassist/Cargo.toml --locked`. Native runtime testing also exercises the packaged component through the real Cinnabar host, including both modes, configuration reloads, denied grants and the maximum 128-player snapshot.

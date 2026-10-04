# Cinnaroids

Open **Cinnaroids.exe** and select **Start Cinnabar**. A compatible client is bundled beside the launcher. **Choose client** selects a separately built client with the required WASM APIs.

Press **Right Shift** in Cinnabar to open or close the module interface. Manage Clicker, Aim assist, Reach, key bindings, and module settings there. **F10** stops all modules. The launcher has no input hooks or module switches; closing it leaves the running Cinnabar session and its module settings alone.

Press **Toggle key** and then a keyboard key to bind the clicker. Right Shift and F10 remain reserved. Escape is assignable during capture and closes the panel otherwise. Closing the panel or changing focus cancels capture.

The launcher starts in neutral dark mode and also supports light mode. Drag its header to move it. Regular top-right controls minimize, maximize or restore, and close it.

Launcher preferences save to `%LOCALAPPDATA%/Cinnaroids/settings.json`. Existing dark/light and client-path preferences migrate from `%LOCALAPPDATA%/CinnabarClicker/settings.json`; the old file is preserved. Module settings are stored separately beside the installed WASM component. [Module setup and behavior](modules.md).

## Build

Windows x64 requires Rust with edition 2024 support and Visual Studio C++ Build Tools with a Windows SDK.

```powershell
cargo build --release --locked
cargo test --release --locked
```

The binary is `target/release/cinnaroids.exe`; distribute it as **Cinnaroids.exe**. GPUI is pinned to 0.2.2 and the MSVC runtime is linked statically. Icons and the module component are embedded.

`Cinnaroids.exe --smoke-test` previews the launcher for three seconds without installing the module, launching Cinnabar, or writing settings. Add `--light` to preview light mode.

The launcher uses Windows' native outer outline with GPUI's existing resize and maximize behavior. The frame helper removes the redundant top border. Windows transparency and graphics support determine the backdrop effect.

The application source and original assets use the MIT license in LICENSE.txt. Dependency notices are in THIRD_PARTY_NOTICES.txt. This independent project is separate from the Cinnabar client.

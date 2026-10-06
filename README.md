# Cinnaroids

Clicker, aim assist, reach, FakeLag, Netherite Finder, Fullbright and Auto Jump Reset for Cinnabar, controlled in-game with **Right Shift**.

![Cinnaroids](assets/screenshot.png)

Open the **Cinnaroids launcher** before or while your installed Cinnabar is running. It attaches automatically to the standard installation; press **Right Shift** to manage modules. **F10** stops all modules. Cinnaroids never starts the game.

Combat keybinds: **F8** Clicker, **R** Aim assist, **V** Reach. **B** toggles Auto Jump Reset in the Movement tab.

Rust + GPUI launcher and a WASM module. Build: `cargo run --release --locked`. [Setup](docs/modules.md).

Install the latest release on **Linux or macOS**:

```sh
curl -fsSL https://github.com/RestartFU/cinnaroids/releases/latest/download/install.sh | sh
```

Installs without sudo to `~/.local/bin/cinnaroids` on Linux or `~/Applications/Cinnaroids.app` on macOS, with architecture detection and checksum verification. Run it again to update. [Installation details](docs/install.md).

Release builds are available for Linux and Windows x86_64, and macOS Intel and Apple Silicon. Linux ships a `.tar.gz`, Windows a standalone `.exe` with license notices in a separate `-notices.zip`, and macOS a `.zip` containing `Cinnaroids.app`. Each download includes a SHA-256 checksum. The Windows executable embeds the module and UI assets and runs directly. Linux requires Vulkan drivers, Fontconfig, Wayland or X11, and xkbcommon. macOS bundles use ad-hoc signing and are not notarized.

On Linux, the launcher watches `$XDG_DATA_HOME/cinnabar/app/bin/bedrock-client` (default `~/.local/share/cinnabar/app/bin/bedrock-client`) and registers modules under `$XDG_CONFIG_HOME/cinnabar` (default `~/.config/cinnabar`). Older installations at `~/.local/bin/bedrock-client` are also supported. On macOS it watches `/Applications/Cinnabar.app/Contents/MacOS/bedrock-client`. Set `CINNABAR_EXECUTABLE` to the absolute path of your installed Cinnabar executable if it lives elsewhere. Attachment requires a Cinnabar build that supports the local module registration protocol; a client without it is detected and reported as incompatible.

To publish, open **Actions → Release → Run workflow** and choose **patch** (default), **minor**, or **major**. The workflow uses the default branch, updates launcher and module manifests, lockfiles and Windows resources, commits and tags `vX.Y.Z`, builds the embedded WASM component and all four native executables, then publishes a GitHub release with `install.sh`. A pushed `vX.Y.Z` tag also releases when it matches the source version. Re-run failed jobs to retry; a new manual run also recovers an unpublished tag at the branch's HEAD without bumping twice. The repository token needs permission to push the release commit and tag to the default branch.

Release automation checks: `python3 -m unittest discover -s scripts/tests -v`.

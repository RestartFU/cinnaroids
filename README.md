# Cinnaroids

Clicker, aim assist, reach, FakeLag, Netherite Finder, Fullbright and Auto Jump Reset for Cinnabar, controlled in-game with **F7**.

![Cinnaroids](assets/screenshot.png)

Download `cinnaroids.wasm` from the [latest release](https://github.com/RestartFU/cinnaroids/releases/latest), then open **Settings → Mods → Import** in an updated Cinnabar. Select the file, review its requested permissions, and enable it. Cinnabar manages loading and settings; no launcher is needed. [Installation and updates](docs/install.md).

**F7** opens the panel. Module shortcuts start unbound, and modules start disabled. Change bindings and preferences in the panel. [Module behavior](docs/modules.md).

The Fold menu keeps every module in one scrollable list. Click a module name to expand its settings; switches remain independent, and Fullbright is a compact toggle-only row. Live summaries stay visible when settings collapse. Menu settings are behind the header’s settings button. Press Escape while setting a module keybind to clear it; Escape cancels capture for the Menu key.

Build with Rust 1.99.0, the `wasm32-unknown-unknown` target, and Python 3.11 or newer:

```sh
python3 scripts/build-cinnaroids.py
cargo test --locked -p cinnaroids-mod
python3 -m unittest discover -s scripts/tests -v
```

The output is `dist/cinnaroids.wasm` and its SHA-256 checksum. The component embeds its Mods-card metadata, requested permissions, icon, Inter Medium panel font, retained dark/light UI catalog and license notices. Packaging validates it and rejects WASI imports. The SDK is pinned in the workspace manifest to Cinnabar commit `8cb8bf5b3bd76fb03de3e5abf04e399b68a68ca2`.

The panel uses Cinnabar’s reversible open/close transitions, accordion motion, hover/toggle feedback and smooth wheel scrolling. Text keeps a steady scale during opening and closing, with the embedded antialiased Inter face. The client’s **Screen animations** preference controls all motion; use a Cinnabar build containing the merged surface-animation support.

Edit the Fold layout in `scripts/generate-panel.py`, then run it to regenerate both theme surfaces. Builds check that the generated files are current.

To publish, open **Actions → Release → Run workflow** and select **patch**, **minor**, or **major**. The workflow bumps the shared workspace version and lockfile, commits and tags `vX.Y.Z`, tests and builds the module, then publishes the WASM and checksum. A matching pushed version tag also releases. Failed jobs can be re-run; a manual run recovers an unpublished tag at the default branch's HEAD without bumping twice.

Cinnaroids source and original assets use the MIT license. The Cinnabar SDK has its own PolyForm Noncommercial license; see [third-party notices](THIRD_PARTY_NOTICES.txt). Cinnaroids is independent of Cinnabar and its maintainers.

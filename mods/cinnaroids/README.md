# Cinnaroids component

Rust WASM modules for Cinnabar's granted local API. Import the finished component through **Settings → Mods → Import**, approve its permissions and enable it. **F7** opens its panel. Module shortcuts start unbound. Preferences and module switches persist across reloads and restarts. Modules with missing required permissions remain off.

From the repository root:

```sh
python3 scripts/build-cinnaroids.py
cargo test --locked --workspace
```

Rust 1.99.0 and `wasm32-unknown-unknown` are pinned in the root toolchain file. The result is `dist/cinnaroids.wasm`, including metadata, requested permissions, an icon and license notices. Packaging validates the component and rejects WASI imports.

The component declares its workspace version, GitHub release repository and exact
latest-release WASM URL. Cinnabar builds with automatic mod updates can download
and install newer stable releases while preserving settings and permissions.
Players can disable automatic installation or all mod checks in **Settings →
General**. Older hosts ignore the optional metadata.

See [module behavior](../../docs/modules.md) and [installation](../../docs/install.md).

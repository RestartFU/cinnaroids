# Cinnaroids component

Rust WASM modules for Cinnabar's granted local API. Import the finished component through **Settings → Mods → Import**, approve its permissions and enable it. **F7** opens its panel and **F10** stops all modules. Preferences persist; enabled switches start off when the component initializes.

From the repository root:

```sh
python3 scripts/build-cinnaroids.py
cargo test --locked -p cinnaroids-mod
```

Rust 1.99.0 and `wasm32-unknown-unknown` are pinned in the root toolchain file. The result is `dist/cinnaroids.wasm`, including metadata, requested permissions, an icon and license notices. Packaging validates the component and rejects WASI imports.

See [module behavior](../../docs/modules.md) and [installation](../../docs/install.md).

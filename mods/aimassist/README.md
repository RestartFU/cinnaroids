# Aim assist component

A deterministic Rust WASM mod for Cinnabar's local gameplay API. It reads remote player positions and submits one bounded camera rotation per frame. There is no AI model, mouse injection, or WASI dependency.

The desktop UI configures enabled state, strength (0–100), and activation (continuous or only while Attack is held). Defaults are disabled, 35% strength, and only while clicking. Targets must be within six blocks and a 30° cone. The component aims at feet + 1.35 blocks, favors the smallest view angle, and retains its target until another is at least 2° closer. Positions do not provide block visibility or team information.

Build from the repository root with `./scripts/build-aimassist.ps1`; test the math with `cargo test --manifest-path mods/aimassist/Cargo.toml -p cinnabar-aimassist-mod --locked`. Rust 1.93.1 and `wasm32-unknown-unknown` are pinned in `rust-toolchain.toml`. The SDK revision is pinned to the gameplay API merge.

The result is `assets/aimassist.component.wasm`. Packaging validates the component and requires exactly one 24-byte initialized configuration block. The 16-byte marker is `CNBR_AIM_CFG_v1!`; bytes 16, 17, and 18 hold enabled, strength, and activation mode, with five zero reserved bytes. The guest reads those values volatile. The desktop changes only this fixed-size payload and atomically replaces the selected component so Cinnabar reloads it.

Cinnabar must be built with `local-mods` and launched with `CINNABAR_MOD_COMPONENT` pointing to the configured file, plus `CINNABAR_MOD_PLAYERS=1` and `CINNABAR_MOD_CAMERA=1`. The grants are explicit host opt-ins. Loss of gameplay input, release in click-only mode, and session/dimension changes reset target selection. Strength zero produces no motion; frame time is clamped to 100 ms and rotation stays within the host's 0.25 radian budget per axis.

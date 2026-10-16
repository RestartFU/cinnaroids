//! Personal Cinnaroids modules managed by the host-rendered in-game panel.

pub mod aim;
#[cfg(target_arch = "wasm32")]
mod guest;
pub mod jump_reset;
mod panel;
pub mod state;

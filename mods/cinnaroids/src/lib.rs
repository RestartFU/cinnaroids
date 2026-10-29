//! Personal Cinnaroids modules, controlled from the component's own in-game panel surface.

pub mod aim;
pub mod auto_totem;
pub mod bindings;
pub mod esp;
pub mod finder;
pub mod ghost;
#[cfg(target_arch = "wasm32")]
mod guest;
pub mod jump_reset;
mod panel;
pub mod state;

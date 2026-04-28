pub mod configuration;
mod format_text;
mod sort;

pub use format_text::format_text;

#[cfg(target_arch = "wasm32")]
mod wasm_plugin;

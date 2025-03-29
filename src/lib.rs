pub mod core;
pub mod error;
pub mod plugins;

pub use core::SsufidCore;

pub use error::Error;
pub use error::PluginError;
pub use error::PluginErrorKind;

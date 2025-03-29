use thiserror::Error;

use crate::core::SsufidPlugin;

#[derive(Debug, Error)]
pub enum Error {
    #[error("File I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error(transparent)]
    Plugin(Box<PluginError>),
}

impl From<PluginError> for Error {
    fn from(err: PluginError) -> Self {
        Error::Plugin(Box::new(err))
    }
}

#[derive(Debug, Error)]
#[error("Error from plugin {plugin}: {kind:?} - {message}")]
pub struct PluginError {
    kind: PluginErrorKind,
    plugin: &'static str,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginErrorKind {
    Request,
    Parse,
    Custom(Box<str>),
    Unknown,
}

impl PluginError {
    pub fn request<T: SsufidPlugin>(message: String) -> Self {
        Self {
            kind: PluginErrorKind::Request,
            plugin: T::IDENTIFIER,
            message,
        }
    }

    pub fn parse<T: SsufidPlugin>(message: String) -> Self {
        Self {
            kind: PluginErrorKind::Parse,
            plugin: T::IDENTIFIER,
            message,
        }
    }

    pub fn custom<T: SsufidPlugin>(name: String, message: String) -> Self {
        Self {
            kind: PluginErrorKind::Custom(name.into()),
            plugin: T::IDENTIFIER,
            message,
        }
    }

    pub fn kind(&self) -> &PluginErrorKind {
        &self.kind
    }

    pub fn plugin(&self) -> &str {
        self.plugin
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

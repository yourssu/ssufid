use thiserror::Error;

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
    plugin: String,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginErrorKind {
    Request,
    Parse,
    Custom,
}

impl PluginError {
    pub fn request(plugin: &str, message: String) -> Self {
        Self {
            kind: PluginErrorKind::Request,
            plugin: plugin.to_string(),
            message,
        }
    }

    pub fn parse(plugin: &str, message: String) -> Self {
        Self {
            kind: PluginErrorKind::Parse,
            plugin: plugin.to_string(),
            message,
        }
    }

    pub fn custom(plugin: &str, message: String) -> Self {
        Self {
            kind: PluginErrorKind::Custom,
            plugin: plugin.to_string(),
            message,
        }
    }
}

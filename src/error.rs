use std::{io, path::PathBuf};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, MudClientError>;

#[derive(Debug, Error)]
pub enum MudClientError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("configuration file {path} is invalid: {source}")]
    ConfigParse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error(
        "merged configuration from {shared} and character profile {character} is invalid: {source}"
    )]
    CharacterConfigParse {
        shared: String,
        character: PathBuf,
        source: Box<toml::de::Error>,
    },
    #[error("configuration is invalid: {0}")]
    ConfigValidation(String),
    #[error("command line argument is invalid: {0}")]
    Cli(String),
    #[error("terminal error: {0}")]
    Terminal(String),
}

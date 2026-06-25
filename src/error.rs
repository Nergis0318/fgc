use std::fmt;

#[derive(Debug)]
pub struct FgcError {
    message: String,
}

impl FgcError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for FgcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FgcError {}

pub type Result<T> = std::result::Result<T, FgcError>;

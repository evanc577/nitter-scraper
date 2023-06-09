use std::process::ExitCode;

#[derive(Debug)]
pub enum NitterError {
    Parse(String),
    Network(String),
    ProtectedAccount,
    SuspendedAccount,
    NotFound,
}

impl std::fmt::Display for NitterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(s) => write!(f, "unable to parse nitter: {}", s),
            Self::Network(s) => write!(f, "unable to send request: {}", s),
            Self::ProtectedAccount => write!(f, "account is protected"),
            Self::SuspendedAccount => write!(f, "account is suspended"),
            Self::NotFound => write!(f, "account not found"),
        }
    }
}

impl NitterError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::ProtectedAccount | Self::SuspendedAccount | Self::NotFound => ExitCode::from(10),
            _ => ExitCode::FAILURE,
        }
    }
}

impl std::error::Error for NitterError {}

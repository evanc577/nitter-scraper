#[derive(Debug)]
pub enum NitterError {
    Parse(String),
    Network(String),
    ProtectedAccount,
}

impl std::fmt::Display for NitterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(s) => write!(f, "unable to parse nitter: {}", s),
            Self::Network(s) => write!(f, "unable to send request: {}", s),
            Self::ProtectedAccount => write!(f, "account is protected"),
        }
    }
}

impl std::error::Error for NitterError {}

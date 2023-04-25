#[derive(Debug)]
pub enum NitterError {
    Parse(String),
    Network(String),
}

impl std::fmt::Display for NitterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(s) => write!(f, "unable to parse nitter: {}", s),
            Self::Network(s) => write!(f, "unable to send request: {}", s),
        }
    }
}

impl std::error::Error for NitterError {}

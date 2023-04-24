#[derive(Debug)]
pub enum NitterError {}

impl std::fmt::Display for NitterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NitterError")
    }
}

impl std::error::Error for NitterError {}

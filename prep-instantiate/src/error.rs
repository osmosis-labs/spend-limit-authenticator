use std::fmt::{self, Display, Formatter};

use error_chain::error_chain;
error_chain! {
    foreign_links {
        Io(std::io::Error);
        HttpRequest(reqwest::Error);
        Json(serde_json::Error);
        Toml(toml::de::Error);
        Join(tokio::task::JoinError);
        DateTimeFormat(time::error::Format);
        Inquire(inquire::InquireError);
        PrepError(PrepError);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepError {
    InvalidState { denom: String },
}
impl std::error::Error for PrepError {}
unsafe impl Send for PrepError {}

impl Display for PrepError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            PrepError::InvalidState { denom } => {
                write!(f, "Invalid state: Tracked denom from previous state has denom that doesn't appear in config: {}", denom)
            }
        }
    }
}

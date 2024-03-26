pub mod error;
pub use error::Result;

pub mod config;
pub use config::Config;

mod router;
pub use router::{get_route, Token};

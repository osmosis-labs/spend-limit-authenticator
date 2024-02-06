mod error;
mod handler;

use handler::*;

pub use {
    authenticate::authenticate, confirm_execution::confirm_execution, error::AuthenticatorError,
    track::track,
};

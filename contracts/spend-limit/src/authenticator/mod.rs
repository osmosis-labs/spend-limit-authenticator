mod composite;
mod error;
mod handler;

use handler::*;

pub use {
    authenticate::authenticate,
    composite::{
        CompositeAuthenticator, CompositeId, CosmwasmAuthenticatorData, SubAuthenticatorData,
    },
    confirm_execution::confirm_execution,
    error::AuthenticatorError,
    on_authenticator_added::on_authenticator_added,
    on_authenticator_removed::on_authenticator_removed,
    track::track,
};

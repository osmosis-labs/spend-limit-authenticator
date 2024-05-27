use cosmwasm_std::{Addr, StdError};
use thiserror::Error;

use super::composite::CompositeAuthenticatorError;

#[derive(Error, Debug, PartialEq)]
pub enum AuthenticatorError {
    #[error("{0}")]
    StdError(#[from] StdError),

    #[error("{0}")]
    CompositeAuthenticatorError(#[from] CompositeAuthenticatorError),

    #[error("Missing authenticator params")]
    MissingAuthenticatorParams,

    #[error("Invalid params: {src}")]
    InvalidAuthenticatorParams {
        #[source]
        src: StdError,
    },

    #[error("Authenticator already exists for account {account} and authenticator id {authenticator_id}")]
    AuthenticatorAlreadyExists {
        account: Addr,
        authenticator_id: String,
    },
}

impl AuthenticatorError {
    pub fn invalid_authenticator_params(src: impl Into<StdError>) -> Self {
        Self::InvalidAuthenticatorParams { src: src.into() }
    }

    pub fn authenticator_already_exists(account: Addr, authenticator_id: &str) -> Self {
        Self::AuthenticatorAlreadyExists {
            account,
            authenticator_id: authenticator_id.to_string(),
        }
    }
}

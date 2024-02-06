use cosmwasm_std::StdError;
use thiserror::Error;

use crate::spend_limit::SpendingKey;

#[derive(Error, Debug, PartialEq)]
pub enum AuthenticatorError {
    #[error("{0}")]
    StdError(#[from] StdError),

    #[error("Missing authenticator params")]
    MissingAuthenticatorParams,

    #[error("Invalid params: {src}")]
    InvalidAuthenticatorParams {
        #[source]
        src: StdError,
    },

    #[error("Transient balances already exists for this key: {key}")]
    DirtyTransientBalances { key: String },
}

impl AuthenticatorError {
    pub fn invalid_authenticator_params(src: impl Into<StdError>) -> Self {
        Self::InvalidAuthenticatorParams { src: src.into() }
    }

    pub fn dirty_transient_balances(key: &SpendingKey) -> Self {
        Self::DirtyTransientBalances {
            key: format!("{:?}", key),
        }
    }
}
pub type AuthenticatorResult<T> = Result<T, AuthenticatorError>;

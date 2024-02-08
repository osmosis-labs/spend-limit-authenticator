use cosmwasm_std::{Addr, StdError};
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

    #[error("Invalid denom: {denom}")]
    InvalidDenom { denom: String },

    #[error("Authenticator already exists for account {account} and subkey {subkey}")]
    AuthenticatorAlreadyExists { account: Addr, subkey: String },

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

    pub fn invalid_denom(denom: &str) -> Self {
        Self::InvalidDenom {
            denom: denom.to_string(),
        }
    }

    pub fn authenticator_already_exists(account: Addr, subkey: &str) -> Self {
        Self::AuthenticatorAlreadyExists {
            account,
            subkey: subkey.to_string(),
        }
    }
}
pub type AuthenticatorResult<T> = Result<T, AuthenticatorError>;

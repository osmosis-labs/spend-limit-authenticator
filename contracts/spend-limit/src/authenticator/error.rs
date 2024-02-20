use cosmwasm_std::{Addr, StdError, Uint64};
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

    #[error("Authenticator already exists for account {account} and authenticator id {authenticator_id}")]
    AuthenticatorAlreadyExists {
        account: Addr,
        authenticator_id: Uint64,
    },

    #[error("PreExec balances already exists for this key: {key}")]
    DirtyPreExecBalances { key: String },
}

impl AuthenticatorError {
    pub fn invalid_authenticator_params(src: impl Into<StdError>) -> Self {
        Self::InvalidAuthenticatorParams { src: src.into() }
    }

    pub fn dirty_pre_exec_balances(key: &SpendingKey) -> Self {
        Self::DirtyPreExecBalances {
            key: format!("{:?}", key),
        }
    }

    pub fn invalid_denom(denom: &str) -> Self {
        Self::InvalidDenom {
            denom: denom.to_string(),
        }
    }

    pub fn authenticator_already_exists(account: Addr, authenticator_id: u64) -> Self {
        Self::AuthenticatorAlreadyExists {
            account,
            authenticator_id: Uint64::from(authenticator_id),
        }
    }
}
pub type AuthenticatorResult<T> = Result<T, AuthenticatorError>;

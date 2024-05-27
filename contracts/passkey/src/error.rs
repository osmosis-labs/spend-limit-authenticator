use thiserror::Error;

use cosmwasm_std::StdError;

use crate::{authenticator::AuthenticatorError, passkey::PasskeyError};

/// Never is a placeholder to ensure we don't return any errors
#[derive(Error, Debug)]
pub enum Never {}

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Authenticator error: {0}")]
    AuthenticatorError(#[from] AuthenticatorError),

    #[error("Spend limit error: {0}")]
    PasskeyError(#[from] PasskeyError),
}

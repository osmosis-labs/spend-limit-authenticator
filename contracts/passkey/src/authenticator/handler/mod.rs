use cosmwasm_std::{from_json, Binary};

use super::AuthenticatorError;
use crate::passkey::PasskeyParams;

pub mod authenticate;
pub mod confirm_execution;
pub mod on_authenticator_added;
pub mod on_authenticator_removed;
pub mod track;

/// Validate and parse the authenticator_params
/// Returns an error if the authenticator_params are missing or invalid
fn validate_and_parse_params(
    authenticator_params: Option<Binary>,
) -> Result<PasskeyParams, AuthenticatorError> {
    // Make sure the authenticator_params are present
    let authenticator_params =
        authenticator_params.ok_or(AuthenticatorError::MissingAuthenticatorParams)?;

    // Make sure the authenticator_params are parsed correctly
    from_json(authenticator_params.as_slice())
        .map_err(AuthenticatorError::invalid_authenticator_params)
}

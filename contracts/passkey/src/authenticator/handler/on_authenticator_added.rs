use cosmwasm_std::{DepsMut, Env, Response};
use osmosis_authenticators::OnAuthenticatorAddedRequest;

use crate::authenticator::{handler::validate_and_parse_params, AuthenticatorError};


pub fn on_authenticator_added(
    _deps: DepsMut,
    _env: Env,
    OnAuthenticatorAddedRequest {
        authenticator_id,
        account,
        authenticator_params,
    }: OnAuthenticatorAddedRequest,
) -> Result<Response, AuthenticatorError> {
    let _ = validate_and_parse_params(authenticator_params)?;

    // Make sure (account, authenticator_id) is not already present in the state
    let _key = (&account, authenticator_id.as_str());
    // ensure!

    // initialize the passkey for this authenticator

    Ok(Response::new().add_attribute("action", "on_authenticator_added"))
}
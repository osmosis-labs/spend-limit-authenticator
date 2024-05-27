use cosmwasm_std::{DepsMut, Env, Response};
use osmosis_authenticators::OnAuthenticatorRemovedRequest;

use crate::authenticator::AuthenticatorError;

pub fn on_authenticator_removed(
    _deps: DepsMut,
    _env: Env,
    OnAuthenticatorRemovedRequest {
        // account,
        // authenticator_id,
        ..
    }: OnAuthenticatorRemovedRequest,
) -> Result<Response, AuthenticatorError> {
    // clean up

    Ok(Response::new().add_attribute("action", "on_authenticator_removed"))
}
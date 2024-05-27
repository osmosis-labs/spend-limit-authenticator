use cosmwasm_std::{DepsMut, Env, Response};
use osmosis_authenticators::AuthenticationRequest;

use crate::{
    error::ContractError,
    passkey::update_and_check_passkey,
};

use super::validate_and_parse_params;

pub fn authenticate(
    deps: DepsMut,
    _env: Env,
    auth_request: AuthenticationRequest,
) -> Result<Response, ContractError> {
    let params = validate_and_parse_params(auth_request.authenticator_params)?;

    let _key = (
        &auth_request.account,
        auth_request.authenticator_id.as_str(),
    );

    update_and_check_passkey(deps, &params)?;

    Ok(Response::new().add_attribute("action", "authenticate"))
}

use cosmwasm_std::{DepsMut, Env, Response};
use osmosis_authenticators::ConfirmExecutionRequest;

use crate::{error::ContractError, passkey::{update_and_check_passkey, PasskeyParams}};

use super::validate_and_parse_params;

pub fn confirm_execution(
    mut deps: DepsMut,
    _env: Env,
    ConfirmExecutionRequest {
        // authenticator_id,
        // account,
        authenticator_params,
        ..
    }: ConfirmExecutionRequest,
) -> Result<Response, ContractError> {
    let params: PasskeyParams = validate_and_parse_params(authenticator_params)?;

    update_and_check_passkey(deps.branch(), &params)?;

    // save

    // clean up

    Ok(Response::new()
        .add_attribute("action", "confirm_execution"))
}

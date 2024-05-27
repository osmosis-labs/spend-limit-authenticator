use crate::error::ContractError;
use cosmwasm_std::{DepsMut, Env, Response};
use osmosis_authenticators::TrackRequest;

use super::validate_and_parse_params;

pub fn track(
    _deps: DepsMut,
    _env: Env,
    TrackRequest {
        account,
        authenticator_id,
        authenticator_params,
        ..
    }: TrackRequest,
) -> Result<Response, ContractError> {
    let _params = validate_and_parse_params(authenticator_params)?;
    let _key = (&account, authenticator_id.as_str());

    // add new fee to untracked spent fee, if confirm execution passed, it will be cleaned up
    // if execution or confirmation failed, it will be accumulated and check at authenticate

    // force update pre_exec_balance, disregard the previous value

    Ok(Response::new().add_attribute("action", "track"))
}
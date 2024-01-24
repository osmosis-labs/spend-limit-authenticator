use cosmwasm_std::{DepsMut, Env, Response};

use crate::ContractError;
use osmosis_authenticators::{AuthenticationResult, TrackRequest};

pub fn track(
    deps: DepsMut,
    _env: Env,
    track_request: TrackRequest,
) -> Result<Response, ContractError> {
    deps.api
        .debug(&format!("track_request {:?}", track_request));

    Ok(Response::new().set_data(AuthenticationResult::Authenticated {}))
}

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{DepsMut, Env, Response};
use osmosis_authenticators::{AuthenticationRequest, AuthenticationResult};

use crate::ContractError;

#[cw_serde]
pub struct AuthenticatorParams {
    pub id: String,
    pub duration: u64,
    pub limit: u128,
}

pub fn authenticate(
    _deps: DepsMut,
    _env: Env,
    _auth_request: AuthenticationRequest,
) -> Result<Response, ContractError> {
    Ok(Response::new().set_data(AuthenticationResult::NotAuthenticated {}))
}

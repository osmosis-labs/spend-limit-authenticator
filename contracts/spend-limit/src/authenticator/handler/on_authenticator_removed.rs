use cosmwasm_std::{DepsMut, Env, Response};
use osmosis_authenticators::OnAuthenticatorRemovedRequest;

use crate::ContractError;

fn on_authenticator_removed(
    deps: DepsMut,
    env: Env,
    OnAuthenticatorRemovedRequest {
        account,
        authenticator_params,
    }: OnAuthenticatorRemovedRequest,
) -> Result<Response, ContractError> {
    Ok(Response::new())
}

#[cfg(test)]
mod tests {
    use super::*;
}

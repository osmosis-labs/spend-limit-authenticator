use cosmwasm_std::{DepsMut, Env, Response};
use osmosis_authenticators::OnAuthenticatorAddedRequest;

use crate::ContractError;

fn on_authenticator_added(
    deps: DepsMut,
    env: Env,
    OnAuthenticatorAddedRequest {
        account,
        authenticator_params,
    }: OnAuthenticatorAddedRequest,
) -> Result<Response, ContractError> {
    Ok(Response::new())
}

#[cfg(test)]
mod tests {
    use super::*;
}

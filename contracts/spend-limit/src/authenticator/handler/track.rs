use cosmwasm_std::{ensure, from_json, DepsMut, Env, Response};

use osmosis_authenticators::TrackRequest;

use crate::{spend_limit::SpendLimitParams, state::TRANSIENT_BALANCES};

use crate::authenticator::error::{AuthenticatorError, AuthenticatorResult};

pub fn track(
    deps: DepsMut,
    _env: Env,
    TrackRequest {
        account,
        authenticator_params,
        ..
    }: TrackRequest,
) -> AuthenticatorResult<Response> {
    let account = account;
    let params = authenticator_params.ok_or(AuthenticatorError::MissingAuthenticatorParams)?;

    let params: SpendLimitParams =
        from_json(params.as_slice()).map_err(AuthenticatorError::invalid_authenticator_params)?;

    let spend_limit_key = (&account, params.subkey.as_str());

    // query all the balances of the account
    let balances = deps.querier.query_all_balances(&account)?;

    // ensure there is no transient balance tracker for this account
    let no_dirty_transient_balance = !TRANSIENT_BALANCES.has(deps.storage, spend_limit_key);
    ensure!(
        no_dirty_transient_balance,
        AuthenticatorError::dirty_transient_balances(&spend_limit_key)
    );

    TRANSIENT_BALANCES.save(deps.storage, spend_limit_key, &balances)?;

    Ok(Response::new())
}

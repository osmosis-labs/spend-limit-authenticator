use cosmwasm_std::{from_json, Decimal, DepsMut, Env, Response};
use osmosis_authenticators::{ConfirmExecutionRequest, ConfirmationResult};

use crate::spend_limit::{calculate_spent_coins, SpendLimitParams};

use crate::state::{SPENDINGS, TRANSIENT_BALANCES};
use crate::ContractError;

use super::AuthenticatorError;

pub fn confirm_execution(
    deps: DepsMut,
    env: Env,
    ConfirmExecutionRequest {
        account,
        authenticator_params,
        ..
    }: ConfirmExecutionRequest,
) -> Result<Response, ContractError> {
    let account = account;
    let params =
        authenticator_params.ok_or_else(|| AuthenticatorError::MissingAuthenticatorParams)?;

    let params: SpendLimitParams =
        from_json(params.as_slice()).map_err(AuthenticatorError::invalid_authenticator_params)?;

    let spend_limit_key = (&account, params.subkey.as_str());

    // get the transient balance for this key
    let pre_exec_balances = TRANSIENT_BALANCES.load(deps.storage, spend_limit_key)?;

    // query all the balances of the account
    let post_exec_balances = deps.querier.query_all_balances(&account)?;

    let spent_coins = calculate_spent_coins(pre_exec_balances, post_exec_balances);

    let mut spending = SPENDINGS.load(deps.storage, spend_limit_key)?;

    for coin in spent_coins.iter() {
        // TODO: query conversion rate
        let conversion_rate = Decimal::one();

        spending = spending.spend(
            coin.amount,
            conversion_rate,
            params.limit.amount,
            &params.reset_period,
            env.block.time,
        )?;
    }

    // save the updated spending
    SPENDINGS.save(deps.storage, spend_limit_key, &spending)?;

    Ok(Response::new().set_data(ConfirmationResult::Confirm {}))
}

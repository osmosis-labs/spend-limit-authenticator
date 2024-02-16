#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response, StdResult,
};
use cw2::set_contract_version;

use crate::authenticator;
use crate::msg::{InstantiateMsg, QueryMsg, SpendingResponse, SpendingsByAccountResponse, SudoMsg};
use crate::price::track_denom;
use crate::spend_limit::Spending;
use crate::state::{PRICE_INFOS, PRICE_RESOLUTION_CONFIG, SPENDINGS};
use crate::ContractError;

const CONTRACT_NAME: &str = "crates.io:spend-limit";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let conf = msg.price_resolution_config;

    PRICE_RESOLUTION_CONFIG.save(deps.storage, &conf)?;

    for tracked_denom in msg.tracked_denoms {
        let denom = tracked_denom.denom;
        let swap_routes = tracked_denom.swap_routes;
        track_denom(
            &PRICE_INFOS,
            deps.branch(),
            &conf,
            &denom,
            env.block.time,
            swap_routes,
        )?;
    }

    Ok(Response::new().add_attribute("action", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        SudoMsg::OnAuthenticatorAdded(on_authenticator_added_request) => {
            authenticator::on_authenticator_added(deps, env, on_authenticator_added_request)
                .map_err(ContractError::from)
        }
        SudoMsg::Authenticate(auth_request) => authenticator::authenticate(deps, env, auth_request),
        SudoMsg::Track(track_request) => {
            authenticator::track(deps, env, track_request).map_err(ContractError::from)
        }
        SudoMsg::ConfirmExecution(confirm_execution_request) => {
            authenticator::confirm_execution(deps, env, confirm_execution_request)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Spending { account, subkey } => {
            let account = deps.api.addr_validate(&account)?;
            to_json_binary(&query_spending(deps, account, subkey)?)
        }
        QueryMsg::SpendingsByAccount { account } => {
            let account = deps.api.addr_validate(&account)?;
            to_json_binary(&query_spendings_by_account(deps, account)?)
        }
    }
}

pub fn query_spending(deps: Deps, account: Addr, subkey: String) -> StdResult<SpendingResponse> {
    let spending = SPENDINGS.load(deps.storage, (&account, subkey.as_str()))?;
    Ok(SpendingResponse { spending })
}

pub fn query_spendings_by_account(
    deps: Deps,
    account: Addr,
) -> StdResult<SpendingsByAccountResponse> {
    // TODO: make sure it has already limited by authenticator per account from go side? (question to team)
    let spendings: Vec<(String, Spending)> = SPENDINGS
        .prefix(&account)
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<(String, Spending)>>>()?;
    Ok(SpendingsByAccountResponse { spendings })
}

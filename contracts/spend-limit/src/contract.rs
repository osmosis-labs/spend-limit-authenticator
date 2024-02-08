#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response, StdError,
    StdResult,
};
use cw2::set_contract_version;

use crate::authenticator;
use crate::msg::{InstantiateMsg, QueryMsg, SpendingResponse, SpendingsByAccountResponse, SudoMsg};
use crate::spend_limit::Spending;
use crate::state::{PRICE_ORACLE_CONTRACT_ADDR, SPENDINGS};
use crate::ContractError;

const CONTRACT_NAME: &str = "crates.io:spend-limit";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, StdError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // TODO: validate the price oracle contract via cw2
    PRICE_ORACLE_CONTRACT_ADDR.save(
        deps.storage,
        &deps.api.addr_validate(&msg.price_oracle_contract_addr)?,
    )?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        SudoMsg::OnAuthenticatorAdded(_) => Ok(Response::default()),
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
            to_json_binary(&query_spending(deps, account, subkey)?)
        }
        QueryMsg::SpendingsByAccount { account } => {
            to_json_binary(&query_spendings_by_account(deps, account)?)
        }
        QueryMsg::PriceOracleContractAddr {} => {
            to_json_binary(&query_price_oracle_contract_addr(deps)?)
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

pub fn query_price_oracle_contract_addr(deps: Deps) -> StdResult<Addr> {
    PRICE_ORACLE_CONTRACT_ADDR.load(deps.storage)
}

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
};
use cw2::set_contract_version;

use crate::authenticator;
use crate::msg::{InstantiateMsg, QueryMsg, SpendLimitDataResponse, SudoMsg};
use crate::state::{DEPRECATED_SPEND_LIMITS, PRICE_ORACLE_CONTRACT_ADDR};
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
        QueryMsg::GetSpendLimitData { account } => {
            to_json_binary(&query_spend_limit(deps, account)?)
        }
        QueryMsg::PriceOracleContractAddr {} => {
            to_json_binary(&query_price_oracle_contract_addr(deps)?)
        }
    }
}

pub fn query_spend_limit(deps: Deps, account: Addr) -> StdResult<SpendLimitDataResponse> {
    let spend_limit_data = DEPRECATED_SPEND_LIMITS.load(deps.storage, account.to_string())?;
    Ok(SpendLimitDataResponse { spend_limit_data })
}

pub fn query_price_oracle_contract_addr(deps: Deps) -> StdResult<Addr> {
    PRICE_ORACLE_CONTRACT_ADDR.load(deps.storage)
}

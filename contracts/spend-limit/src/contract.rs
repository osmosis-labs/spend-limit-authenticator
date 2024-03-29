#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, to_json_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdResult,
};
use cw2::set_contract_version;

use crate::authenticator;
use crate::msg::{InstantiateMsg, QueryMsg, SpendingResponse, SpendingsByAccountResponse, SudoMsg};
use crate::price::track_denom;
use crate::spend_limit::SpendLimitError;
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

    let supply = deps.querier.query_supply(conf.quote_denom.clone())?;
    // make sure the quote_denom has a non-zero supply
    ensure!(
        !supply.amount.is_zero(),
        ContractError::InvalidDenom {
            denom: conf.quote_denom
        }
    );

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
        SudoMsg::OnAuthenticatorRemoved(on_authenticator_removed_request) => {
            authenticator::on_authenticator_removed(deps, env, on_authenticator_removed_request)
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
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Spending {
            account,
            authenticator_id,
        } => {
            let account = deps.api.addr_validate(&account)?;
            to_json_binary(&query_spending(deps, account, authenticator_id)?)
        }
        QueryMsg::SpendingsByAccount { account } => {
            let account = deps.api.addr_validate(&account)?;
            to_json_binary(&query_spendings_by_account(deps, account)?)
        }
    }
    .map_err(ContractError::from)
}

// TODO: check period, if has changed, reset value_spent_in_period to 0
pub fn query_spending(
    deps: Deps,
    account: Addr,
    authenticator_id: String,
) -> Result<SpendingResponse, ContractError> {
    match SPENDINGS.may_load(deps.storage, (&account, authenticator_id.as_str()))? {
        Some(spending) => Ok(SpendingResponse { spending }),
        None => Err(SpendLimitError::SpendLimitNotFound {
            address: account,
            authenticator_id,
        }
        .into()),
    }
}

pub fn query_spendings_by_account(
    deps: Deps,
    account: Addr,
) -> Result<SpendingsByAccountResponse, ContractError> {
    let spendings = SPENDINGS
        .prefix(&account)
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?;
    Ok(SpendingsByAccountResponse { spendings })
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        from_json,
        testing::{mock_dependencies, mock_dependencies_with_balances, mock_env, mock_info},
        Coin, Uint128, Uint64,
    };
    use osmosis_authenticators::{
        Any, AuthenticationRequest, ConfirmExecutionRequest, OnAuthenticatorAddedRequest,
        OnAuthenticatorRemovedRequest, SignModeTxData, SignatureData, TrackRequest, TxData,
    };
    use osmosis_std::types::cosmos::bank::v1beta1::MsgSend;

    use crate::{
        price::PriceResolutionConfig,
        spend_limit::{Period, SpendLimitParams, Spending},
    };

    use super::*;

    const UUSDC: &str = "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4";

    #[test]
    fn test_happy_path() {
        let mut deps = mock_dependencies_with_balances(&[
            (&"creator".to_string(), &[Coin::new(1000, UUSDC)]),
            (
                &"limited_account".to_string(),
                &[Coin::new(2_000_000, UUSDC)],
            ),
            (&"recipient".to_string(), &[]),
        ]);
        let msg = InstantiateMsg {
            price_resolution_config: PriceResolutionConfig {
                quote_denom: UUSDC.to_string(),
                staleness_threshold: Uint64::from(3_600_000_000u64),
                twap_duration: Uint64::from(3_600_000_000u64),
            },
            tracked_denoms: vec![],
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let authenticator_params = to_json_binary(&SpendLimitParams {
            limit: Uint128::from(1_000_000u128),
            reset_period: Period::Day,
            time_limit: None,
        })
        .unwrap();

        // add authenticator
        sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::OnAuthenticatorAdded(OnAuthenticatorAddedRequest {
                account: Addr::unchecked("limited_account"),
                authenticator_id: "2".to_string(),
                authenticator_params: Some(authenticator_params.clone()),
            }),
        )
        .unwrap();

        let msg = Any {
            type_url: MsgSend::TYPE_URL.to_string(),
            value: Binary::from(
                MsgSend {
                    from_address: "limited_account".to_string(),
                    to_address: "recipient".to_string(),
                    amount: vec![Coin::new(1_000_000, UUSDC).into()],
                }
                .to_proto_bytes(),
            ),
        };

        // authenticate
        sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::Authenticate(AuthenticationRequest {
                authenticator_id: "2".to_string(),
                account: Addr::unchecked("limited_account"),
                fee_payer: Addr::unchecked("limited_account"),
                msg: msg.clone(),
                msg_index: 0,
                signature: Binary::default(),
                sign_mode_tx_data: SignModeTxData {
                    sign_mode_direct: Binary::default(),
                    sign_mode_textual: None,
                },
                tx_data: TxData {
                    chain_id: "osmosis-1".to_string(),
                    account_number: 0,
                    sequence: 0,
                    timeout_height: 0,
                    msgs: vec![],
                    memo: "".to_string(),
                },
                signature_data: SignatureData {
                    signers: vec![],
                    signatures: vec![],
                },
                simulate: false,
                authenticator_params: Some(authenticator_params.clone()),
            }),
        )
        .unwrap();

        // track
        sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::Track(TrackRequest {
                account: Addr::unchecked("limited_account"),
                fee_payer: Addr::unchecked("limited_account"),
                authenticator_id: "2".to_string(),
                msg: msg.clone(),
                msg_index: 0,
                authenticator_params: Some(authenticator_params.clone()),
            }),
        )
        .unwrap();

        // simulate execute bank send
        deps.querier
            .update_balance("limited_account", vec![Coin::new(1_000_001, UUSDC).into()]);

        // confirm execution
        sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::ConfirmExecution(ConfirmExecutionRequest {
                authenticator_id: "2".to_string(),
                account: Addr::unchecked("limited_account"),
                fee_payer: Addr::unchecked("limited_account"),
                msg: msg.clone(),
                msg_index: 0,
                authenticator_params: Some(authenticator_params.clone()),
            }),
        )
        .unwrap();

        // query spending
        let spending = from_json::<SpendingResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::Spending {
                    account: "limited_account".to_string(),
                    authenticator_id: "2".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            spending,
            SpendingResponse {
                spending: Spending {
                    value_spent_in_period: Uint128::from(999_999u128),
                    last_spent_at: mock_env().block.time
                }
            }
        );

        // query spendings by account
        let spendings = from_json::<SpendingsByAccountResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::SpendingsByAccount {
                    account: "limited_account".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            spendings,
            SpendingsByAccountResponse {
                spendings: vec![(
                    "2".to_string(),
                    Spending {
                        value_spent_in_period: Uint128::from(999_999u128),
                        last_spent_at: mock_env().block.time
                    }
                )]
            }
        );

        // remove authenticator
        sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::OnAuthenticatorRemoved(OnAuthenticatorRemovedRequest {
                account: Addr::unchecked("limited_account"),
                authenticator_id: "2".to_string(),
                authenticator_params: Some(authenticator_params),
            }),
        )
        .unwrap();

        // query spending
        let err = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::Spending {
                account: "limited_account".to_string(),
                authenticator_id: "2".to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(
            err,
            SpendLimitError::SpendLimitNotFound {
                address: Addr::unchecked("limited_account"),
                authenticator_id: "2".to_string(),
            }
            .into()
        );

        // query spendings by account
        let spendings = from_json::<SpendingsByAccountResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::SpendingsByAccount {
                    account: "limited_account".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(spendings, SpendingsByAccountResponse { spendings: vec![] });
    }

    #[test]
    fn test_invalid_denom() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            price_resolution_config: PriceResolutionConfig {
                quote_denom: "uinvalid".to_string(),
                staleness_threshold: Uint64::from(3_600_000_000u64),
                twap_duration: Uint64::from(3_600_000_000u64),
            },
            tracked_denoms: vec![],
        };
        let info = mock_info("creator", &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            res,
            ContractError::InvalidDenom {
                denom: "uinvalid".to_string()
            }
            .into()
        );
    }
}

use crate::admin::Admin;
use crate::authenticator::{self};
use crate::msg::{
    AdminCandidateResponse, AdminResponse, DenomRemovalTarget, ExecuteMsg, InstantiateMsg,
    PriceResolutionConfigResponse, QueryMsg, SpendingResponse, SpendingsByAccountResponse, SudoMsg,
    TrackedDenom, TrackedDenomsResponse,
};
use crate::price::{track_denom, PriceResolutionConfig};
use crate::spend_limit::{updated_spending, SpendLimitError};
use crate::state::{ADMIN, PRICE_INFOS, PRICE_RESOLUTION_CONFIG, SPENDINGS, UNTRACKED_SPENT_FEES};
use crate::ContractError;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, to_json_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdResult, Storage, Timestamp, Uint64,
};
use cw2::set_contract_version;
use cw_storage_plus::Bound;

const CONTRACT_NAME: &str = "crates.io:spend-limit";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const MAX_LIMIT: u32 = 100;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // save admin if set
    if let Some(admin) = msg.admin {
        let admin = deps.api.addr_validate(&admin)?;
        ADMIN.save(deps.storage, &Admin::new(admin))?;
    }

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
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::SetPriceResolutionParams {
            staleness_threshold,
            twap_duration,
        } => set_price_resolution_config(deps, info, staleness_threshold, twap_duration),
        ExecuteMsg::RemoveTrackedDenoms { target } => remove_tracked_denoms(deps, info, target),
        ExecuteMsg::SetTrackedDenoms { tracked_denoms } => {
            set_tracked_denoms(deps, env, info, tracked_denoms)
        }
        ExecuteMsg::TransferAdmin { address } => transfer_admin(deps, info, address),
        ExecuteMsg::ClaimAdminTransfer {} => claim_admin_transfer(deps, info),
        ExecuteMsg::RejectAdminTransfer {} => reject_admin_transfer(deps, info),
        ExecuteMsg::CancelAdminTransfer {} => cancel_admin_transfer(deps, info),
        ExecuteMsg::RevokeAdmin {} => revoke_admin(deps, info),
    }
}

fn set_price_resolution_config(
    deps: DepsMut,
    info: MessageInfo,
    staleness_threshold: Uint64,
    twap_duration: Uint64,
) -> Result<Response, ContractError> {
    authorize_admin(deps.storage, &info.sender)?;

    PRICE_RESOLUTION_CONFIG.update(deps.storage, |conf| -> Result<_, ContractError> {
        Ok(PriceResolutionConfig {
            quote_denom: conf.quote_denom,
            staleness_threshold,
            twap_duration,
        })
    })?;

    Ok(Response::new().add_attribute("action", "set_price_resolution_config"))
}

fn remove_tracked_denoms(
    deps: DepsMut,

    info: MessageInfo,
    target: DenomRemovalTarget,
) -> Result<Response, ContractError> {
    authorize_admin(deps.storage, &info.sender)?;

    match target {
        DenomRemovalTarget::All => PRICE_INFOS.clear(deps.storage),
        DenomRemovalTarget::Partial(denoms) => {
            for denom in denoms {
                PRICE_INFOS.remove(deps.storage, &denom);
            }
        }
    }

    Ok(Response::new().add_attribute("action", "remove_tracked_denoms"))
}

fn set_tracked_denoms(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    tracked_denoms: Vec<TrackedDenom>,
) -> Result<Response, ContractError> {
    authorize_admin(deps.storage, &info.sender)?;

    let conf = PRICE_RESOLUTION_CONFIG.load(deps.storage)?;

    for tracked_denom in tracked_denoms {
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

    Ok(Response::new().add_attribute("action", "set_tracked_denoms"))
}

fn transfer_admin(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    let candidate = deps.api.addr_validate(&address)?;

    update_admin(deps.storage, |admin| {
        admin.authorized_transfer_admin(&info.sender, candidate)
    })?;

    Ok(Response::new().add_attribute("action", "transfer_admin"))
}

fn claim_admin_transfer(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    update_admin(deps.storage, |admin| {
        admin.authorized_claim_admin_transfer(&info.sender)
    })?;

    Ok(Response::new().add_attribute("action", "claim_admin"))
}

fn reject_admin_transfer(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    update_admin(deps.storage, |admin| {
        admin.authorized_reject_admin_transfer(&info.sender)
    })?;

    Ok(Response::new().add_attribute("action", "reject_admin"))
}

fn cancel_admin_transfer(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    update_admin(deps.storage, |admin| {
        admin.authorized_cancel_admin_transfer(&info.sender)
    })?;

    Ok(Response::new().add_attribute("action", "cancel_admin"))
}

fn revoke_admin(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    update_admin(deps.storage, |admin| {
        admin.authorized_revoke_admin(&info.sender)
    })?;

    Ok(Response::new().add_attribute("action", "revoke_admin"))
}

fn update_admin(
    store: &mut dyn Storage,
    action: impl FnOnce(Admin) -> Result<Admin, ContractError>,
) -> Result<(), ContractError> {
    let admin = ADMIN.may_load(store)?.unwrap_or(Admin::None);

    ADMIN.save(store, &action(admin)?)?;

    Ok(())
}

fn authorize_admin(store: &mut dyn Storage, sender: &Addr) -> Result<(), ContractError> {
    let admin = ADMIN.may_load(store)?.unwrap_or(Admin::None);
    admin.authorize_admin(sender)
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
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::PriceResolutionConfig {} => to_json_binary(&PriceResolutionConfigResponse {
            price_resolution_config: PRICE_RESOLUTION_CONFIG.load(deps.storage)?,
        }),
        QueryMsg::TrackedDenoms { start_after, limit } => {
            to_json_binary(&query_tracked_denoms(deps, start_after, limit)?)
        }
        QueryMsg::Spending {
            account,
            authenticator_id,
        } => {
            let account = deps.api.addr_validate(&account)?;
            to_json_binary(&query_spending(
                deps,
                account,
                authenticator_id,
                env.block.time,
            )?)
        }
        QueryMsg::SpendingsByAccount { account } => {
            let account = deps.api.addr_validate(&account)?;
            to_json_binary(&query_spendings_by_account(deps, account, env.block.time)?)
        }
        QueryMsg::Admin {} => to_json_binary(&AdminResponse {
            admin: ADMIN
                .may_load(deps.storage)?
                .and_then(|a| a.admin_once())
                .map(|a| a.to_string()),
        }),
        QueryMsg::AdminCandidate {} => to_json_binary(&AdminCandidateResponse {
            candidate: ADMIN
                .may_load(deps.storage)?
                .and_then(|a| a.candidate_once())
                .map(|a| a.to_string()),
        }),
    }
    .map_err(ContractError::from)
}

pub fn query_spending(
    deps: Deps,
    account: Addr,
    authenticator_id: String,
    at: Timestamp,
) -> Result<SpendingResponse, ContractError> {
    match SPENDINGS.may_load(deps.storage, (&account, authenticator_id.as_str()))? {
        Some(spending) => Ok(SpendingResponse {
            spending: updated_spending(
                deps,
                &PRICE_INFOS,
                &UNTRACKED_SPENT_FEES,
                &PRICE_RESOLUTION_CONFIG.load(deps.storage)?,
                &account,
                &authenticator_id,
                at,
                spending,
            )?,
        }),
        None => Err(SpendLimitError::SpendLimitNotFound {
            address: account,
            authenticator_id,
        }
        .into()),
    }
}

pub fn query_tracked_denoms(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> Result<TrackedDenomsResponse, ContractError> {
    let min = start_after.as_ref().map(|s| Bound::exclusive(s.as_str()));
    let limit = limit.unwrap_or(MAX_LIMIT).min(MAX_LIMIT) as usize;

    let tracked_denoms = PRICE_INFOS
        .range(deps.storage, min, None, Order::Ascending)
        .take(limit)
        .map(|item| -> StdResult<TrackedDenom> {
            let (denom, info) = item?;
            Ok(TrackedDenom {
                denom: denom.to_string(),
                swap_routes: info.swap_routes,
            })
        })
        .collect::<StdResult<_>>()?;

    Ok(TrackedDenomsResponse { tracked_denoms })
}

pub fn query_spendings_by_account(
    deps: Deps,
    account: Addr,
    at: Timestamp,
) -> Result<SpendingsByAccountResponse, ContractError> {
    let spendings = SPENDINGS
        .prefix(&account)
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| {
            let (authenticator_id, spending) = item?;
            let conf = PRICE_RESOLUTION_CONFIG.load(deps.storage)?;
            let spending = updated_spending(
                deps,
                &PRICE_INFOS,
                &UNTRACKED_SPENT_FEES,
                &conf,
                &account,
                &authenticator_id,
                at,
                spending,
            )?;
            Ok((authenticator_id, spending))
        })
        .collect::<Result<Vec<_>, ContractError>>()?;
    Ok(SpendingsByAccountResponse { spendings })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use authenticator::SubAuthenticatorData;
    use cosmwasm_std::{
        from_json,
        testing::{mock_dependencies, mock_dependencies_with_balances, mock_env, mock_info},
        to_json_vec, BlockInfo, Coin, ContractResult, Uint128, Uint64,
    };
    use osmosis_authenticators::{
        Any, AuthenticationRequest, ConfirmExecutionRequest, OnAuthenticatorAddedRequest,
        OnAuthenticatorRemovedRequest, SignModeTxData, SignatureData, TrackRequest, TxData,
    };
    use osmosis_std::types::{
        cosmos::bank::v1beta1::MsgSend,
        osmosis::{
            poolmanager::v1beta1::SwapAmountInRoute,
            smartaccount::v1beta1::{AccountAuthenticator, GetAuthenticatorResponse},
            twap::v1beta1::ArithmeticTwapToNowResponse,
        },
    };

    use crate::{
        authenticator::CosmwasmAuthenticatorData,
        fee::UntrackedSpentFee,
        period::Period,
        state::UNTRACKED_SPENT_FEES,
        test_helper::mock_stargate_querier::{
            arithmetic_twap_to_now_query_handler, get_authenticator_query_handler,
            mock_dependencies_with_stargate_querier,
        },
    };
    use crate::{
        price::PriceResolutionConfig,
        spend_limit::{SpendLimitParams, Spending},
    };

    use super::*;

    const UUSDC: &str = "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4";

    #[test]
    fn test_happy_path() {
        let params = SpendLimitParams {
            limit: Uint128::from(1_000_000u128),
            reset_period: Period::Day,
            time_limit: None,
        };

        let params_for_querier_setup = params.clone();
        let mut deps = mock_dependencies_with_stargate_querier(
            &[
                ("creator", &[Coin::new(1000, UUSDC)]),
                ("limited_account", &[Coin::new(2_000_000, UUSDC)]),
                ("recipient", &[]),
            ],
            get_authenticator_query_handler(Box::new(move |req| {
                let account = req.account.as_str();
                let authenticator_id = req.authenticator_id;
                match (account, authenticator_id) {
                    ("limited_account", 2) => ContractResult::Ok(GetAuthenticatorResponse {
                        account_authenticator: Some(AccountAuthenticator {
                            id: 2,
                            r#type: "CosmWasmAuthenticatorV1".to_string(),
                            config: to_json_vec(&CosmwasmAuthenticatorData {
                                contract: mock_env().contract.address.to_string(),
                                params: to_json_vec(&params_for_querier_setup).unwrap(),
                            })
                            .unwrap(),
                        }),
                    }),
                    _ => ContractResult::Err("not found".to_string()),
                }
            })),
        );
        let msg = InstantiateMsg {
            price_resolution_config: PriceResolutionConfig {
                quote_denom: UUSDC.to_string(),
                staleness_threshold: Uint64::from(3_600_000_000u64),
                twap_duration: Uint64::from(3_600_000_000u64),
            },
            tracked_denoms: vec![],
            admin: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let authenticator_params = to_json_binary(&params).unwrap();

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
                fee_granter: None,
                fee: vec![],
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
                fee_granter: None,
                fee: vec![],
                authenticator_id: "2".to_string(),
                msg: msg.clone(),
                msg_index: 0,
                authenticator_params: Some(authenticator_params.clone()),
            }),
        )
        .unwrap();

        // simulate execute bank send
        deps.querier
            .update_balance("limited_account", vec![Coin::new(1_000_001, UUSDC)]);

        // confirm execution
        sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::ConfirmExecution(ConfirmExecutionRequest {
                authenticator_id: "2".to_string(),
                account: Addr::unchecked("limited_account"),
                fee_payer: Addr::unchecked("limited_account"),
                fee_granter: None,
                fee: vec![],
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
            admin: None,
        };
        let info = mock_info("creator", &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            res,
            ContractError::InvalidDenom {
                denom: "uinvalid".to_string()
            }
        );
    }

    #[test]
    fn test_query_spendings() {
        let params_map: BTreeMap<(&str, &str), SpendLimitParams> = vec![
            (
                ("addr_a", "1"),
                SpendLimitParams {
                    limit: Uint128::from(1_000_000u128),
                    reset_period: Period::Day,
                    time_limit: None,
                },
            ),
            (
                ("addr_a", "2.1.0"),
                SpendLimitParams {
                    limit: Uint128::from(2_000_000u128),
                    reset_period: Period::Week,
                    time_limit: None,
                },
            ),
            (
                ("addr_b", "66"),
                SpendLimitParams {
                    limit: Uint128::from(1_000_000u128),
                    reset_period: Period::Month,
                    time_limit: None,
                },
            ),
        ]
        .into_iter()
        .collect();

        let params_for_querier_setup = params_map.clone();

        // setup query handler
        let mut deps = mock_dependencies_with_stargate_querier(
            &[],
            get_authenticator_query_handler(Box::new(move |req| {
                let account = req.account.as_str();
                let authenticator_id = req.authenticator_id;
                match (account, authenticator_id) {
                    ("addr_a", 1) => ContractResult::Ok(GetAuthenticatorResponse {
                        account_authenticator: Some(AccountAuthenticator {
                            id: 2,
                            r#type: "CosmWasmAuthenticatorV1".to_string(),
                            config: to_json_vec(&CosmwasmAuthenticatorData {
                                contract: mock_env().contract.address.to_string(),
                                params: to_json_vec(&params_for_querier_setup[&("addr_a", "1")])
                                    .unwrap(),
                            })
                            .unwrap(),
                        }),
                    }),
                    ("addr_a", 2) => ContractResult::Ok(GetAuthenticatorResponse {
                        account_authenticator: Some(AccountAuthenticator {
                            id: 2,
                            r#type: "AnyOf".to_string(),
                            config: to_json_vec(&[
                                SubAuthenticatorData {
                                    r#type: "Dummy".to_string(),
                                    config: vec![],
                                },
                                SubAuthenticatorData {
                                    r#type: "AllOf".to_string(),
                                    config: to_json_vec(&[SubAuthenticatorData {
                                        r#type: "CosmWasmAuthenticatorV1".to_string(),
                                        config: to_json_vec(&CosmwasmAuthenticatorData {
                                            contract: mock_env().contract.address.to_string(),
                                            params: to_json_vec(
                                                &params_for_querier_setup[&("addr_a", "2.1.0")],
                                            )
                                            .unwrap(),
                                        })
                                        .unwrap(),
                                    }])
                                    .unwrap(),
                                },
                            ])
                            .unwrap(),
                        }),
                    }),
                    ("addr_b", 66) => ContractResult::Ok(GetAuthenticatorResponse {
                        account_authenticator: Some(AccountAuthenticator {
                            id: 2,
                            r#type: "CosmWasmAuthenticatorV1".to_string(),
                            config: to_json_vec(&CosmwasmAuthenticatorData {
                                contract: mock_env().contract.address.to_string(),
                                params: to_json_vec(&params_for_querier_setup[&("addr_b", "66")])
                                    .unwrap(),
                            })
                            .unwrap(),
                        }),
                    }),
                    _ => ContractResult::Err("not found".to_string()),
                }
            })),
        );

        PRICE_RESOLUTION_CONFIG
            .save(
                &mut deps.storage,
                &PriceResolutionConfig {
                    quote_denom: "uosmo".to_string(),
                    staleness_threshold: Uint64::from(3_600_000_000u64),
                    twap_duration: Uint64::from(3_600_000_000u64),
                },
            )
            .unwrap();

        // setup states that correspond to the query hanlders
        let mock_spending = Spending {
            value_spent_in_period: 999_999u128.into(),
            last_spent_at: mock_env().block.time,
        };
        for ((account, authenticator_id), _) in params_map {
            SPENDINGS
                .save(
                    &mut deps.storage,
                    (&Addr::unchecked(account), authenticator_id),
                    &mock_spending,
                )
                .unwrap();
        }

        // test query with both single and per account

        let SpendingResponse { spending } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::Spending {
                    account: "addr_a".to_string(),
                    authenticator_id: "1".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(spending, mock_spending);

        // test reset
        // after 1 day "a, 1" reset
        let SpendingResponse { spending } = from_json(
            query(
                deps.as_ref(),
                mock_env_with_additional_days(1),
                QueryMsg::Spending {
                    account: "addr_a".to_string(),
                    authenticator_id: "1".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        let reset_spending = Spending {
            value_spent_in_period: 0u128.into(),
            last_spent_at: mock_env().block.time,
        };

        assert_eq!(spending, reset_spending,);

        // "a, 2.1.0" not reset
        let SpendingResponse { spending } = from_json(
            query(
                deps.as_ref(),
                mock_env_with_additional_days(1),
                QueryMsg::Spending {
                    account: "addr_a".to_string(),
                    authenticator_id: "2.1.0".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(spending, mock_spending);

        // add a week in
        // after 1 week "a, 2.1.0" reset
        let SpendingResponse { spending } = from_json(
            query(
                deps.as_ref(),
                mock_env_with_additional_days(7),
                QueryMsg::Spending {
                    account: "addr_a".to_string(),
                    authenticator_id: "2.1.0".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(spending, reset_spending);

        // "b, 66" not reset
        let SpendingResponse { spending } = from_json(
            query(
                deps.as_ref(),
                mock_env_with_additional_days(7),
                QueryMsg::Spending {
                    account: "addr_b".to_string(),
                    authenticator_id: "66".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(spending, mock_spending);

        // add a month in
        // after 1 month "b, 66" reset
        let SpendingResponse { spending } = from_json(
            query(
                deps.as_ref(),
                mock_env_with_additional_days(30),
                QueryMsg::Spending {
                    account: "addr_b".to_string(),
                    authenticator_id: "66".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(spending, reset_spending);

        // query for account
        let SpendingsByAccountResponse { spendings } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::SpendingsByAccount {
                    account: "addr_a".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            spendings,
            vec![
                ("1".to_string(), mock_spending.clone()),
                ("2.1.0".to_string(), mock_spending.clone())
            ]
        );

        // add day in
        // after 1 day "a, 1" reset
        let SpendingsByAccountResponse { spendings } = from_json(
            query(
                deps.as_ref(),
                mock_env_with_additional_days(1),
                QueryMsg::SpendingsByAccount {
                    account: "addr_a".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            spendings,
            vec![
                ("1".to_string(), reset_spending.clone()),
                ("2.1.0".to_string(), mock_spending.clone())
            ]
        );

        // add untracked spent fees to "a"

        let fee = vec![Coin::new(100, "uosmo")];

        UNTRACKED_SPENT_FEES
            .save(
                &mut deps.storage,
                (&Addr::unchecked("addr_a"), "1"),
                &UntrackedSpentFee {
                    fee: fee.clone(),
                    updated_at: mock_env().block.time,
                },
            )
            .unwrap();

        UNTRACKED_SPENT_FEES
            .save(
                &mut deps.storage,
                (&Addr::unchecked("addr_a"), "2.1.0"),
                &UntrackedSpentFee {
                    fee,
                    updated_at: mock_env().block.time,
                },
            )
            .unwrap();

        let mock_spending_with_fee = Spending {
            value_spent_in_period: mock_spending.value_spent_in_period + Uint128::from(100u128),
            last_spent_at: mock_env().block.time,
        };

        // query spending
        let SpendingResponse { spending } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::Spending {
                    account: "addr_a".to_string(),
                    authenticator_id: "1".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(spending, mock_spending_with_fee);

        // reset after a day in
        let SpendingResponse { spending } = from_json(
            query(
                deps.as_ref(),
                mock_env_with_additional_days(1),
                QueryMsg::Spending {
                    account: "addr_a".to_string(),
                    authenticator_id: "1".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(spending, reset_spending);
    }

    fn mock_env_with_additional_days(days: u64) -> Env {
        Env {
            block: BlockInfo {
                height: mock_env().block.height + days * 10000,
                time: mock_env().block.time.plus_days(days),
                chain_id: mock_env().block.chain_id,
            },
            transaction: mock_env().transaction,
            contract: mock_env().contract,
        }
    }

    #[test]
    fn test_set_price_resolution_config() {
        let mut deps = mock_dependencies_with_balances(&[("creator", &[Coin::new(100000, UUSDC)])]);

        let init_config = PriceResolutionConfig {
            quote_denom: UUSDC.to_string(),
            staleness_threshold: Uint64::from(3_600_000_000u64),
            twap_duration: Uint64::from(3_600_000_000u64),
        };
        let msg = InstantiateMsg {
            price_resolution_config: init_config.clone(),
            tracked_denoms: vec![],
            admin: Some(String::from("admin")),
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let PriceResolutionConfigResponse {
            price_resolution_config,
        } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::PriceResolutionConfig {},
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(price_resolution_config, init_config);

        let new_config = PriceResolutionConfig {
            quote_denom: UUSDC.to_string(),
            staleness_threshold: Uint64::from(7_200_000_000u64),
            twap_duration: Uint64::from(7_200_000_000u64),
        };
        let msg = ExecuteMsg::SetPriceResolutionParams {
            staleness_threshold: new_config.staleness_threshold,
            twap_duration: new_config.twap_duration,
        };
        let info = mock_info("non_admin", &[]);
        let err = execute(deps.as_mut(), mock_env(), info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let info = mock_info("admin", &[]);
        let _ = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let PriceResolutionConfigResponse {
            price_resolution_config,
        } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::PriceResolutionConfig {},
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(price_resolution_config, new_config);
    }

    #[test]
    fn test_no_admin() {
        let mut deps =
            mock_dependencies_with_balances(&[("creator", &[Coin::new(100000000, UUSDC)])]);

        let msg = InstantiateMsg {
            price_resolution_config: PriceResolutionConfig {
                quote_denom: UUSDC.to_string(),
                staleness_threshold: Uint64::from(3_600_000_000u64),
                twap_duration: Uint64::from(3_600_000_000u64),
            },
            tracked_denoms: vec![],
            admin: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let (admin, candidate) = query_admin_and_candidate(deps.as_ref());

        assert_eq!(admin, None);
        assert_eq!(candidate, None);
    }

    #[test]
    fn test_admin() {
        let mut deps =
            mock_dependencies_with_balances(&[("creator", &[Coin::new(100000000, UUSDC)])]);

        let msg = InstantiateMsg {
            price_resolution_config: PriceResolutionConfig {
                quote_denom: UUSDC.to_string(),
                staleness_threshold: Uint64::from(3_600_000_000u64),
                twap_duration: Uint64::from(3_600_000_000u64),
            },
            tracked_denoms: vec![],
            admin: Some("admin".to_string()),
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let (admin, candidate) = query_admin_and_candidate(deps.as_ref());

        assert_eq!(admin, Some("admin".to_string()));
        assert_eq!(candidate, None);

        // non admin can't transfer admin
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::TransferAdmin {
            address: "new_admin".to_string(),
        };

        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        // admin can transfer admin
        let info = mock_info("admin", &[]);
        let msg = ExecuteMsg::TransferAdmin {
            address: "new_admin".to_string(),
        };

        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let (admin, candidate) = query_admin_and_candidate(deps.as_ref());
        assert_eq!(admin, Some("admin".to_string()));
        assert_eq!(candidate, Some("new_admin".to_string()));

        // non candidate can't claim admin
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::ClaimAdminTransfer {};

        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        // candidate can claim admin
        let info = mock_info("new_admin", &[]);
        let msg = ExecuteMsg::ClaimAdminTransfer {};

        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let (admin, candidate) = query_admin_and_candidate(deps.as_ref());
        assert_eq!(admin, Some("new_admin".to_string()));
        assert_eq!(candidate, None);

        // transfer again
        let info = mock_info("new_admin", &[]);
        let msg = ExecuteMsg::TransferAdmin {
            address: "new_admin_2".to_string(),
        };

        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let (admin, candidate) = query_admin_and_candidate(deps.as_ref());
        assert_eq!(admin, Some("new_admin".to_string()));
        assert_eq!(candidate, Some("new_admin_2".to_string()));

        // only candidate can reject admin transfer
        let info = mock_info("new_admin", &[]);
        let msg = ExecuteMsg::RejectAdminTransfer {};

        let res = execute(deps.as_mut(), mock_env(), info, msg);

        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        // candidate can reject admin transfer
        let info = mock_info("new_admin_2", &[]);
        let msg = ExecuteMsg::RejectAdminTransfer {};

        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let (admin, candidate) = query_admin_and_candidate(deps.as_ref());
        assert_eq!(admin, Some("new_admin".to_string()));
        assert_eq!(candidate, None);

        // transfer again
        let info = mock_info("new_admin", &[]);
        let msg = ExecuteMsg::TransferAdmin {
            address: "new_admin_2".to_string(),
        };

        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let (admin, candidate) = query_admin_and_candidate(deps.as_ref());
        assert_eq!(admin, Some("new_admin".to_string()));
        assert_eq!(candidate, Some("new_admin_2".to_string()));

        // only admin can cancel admin transfer
        let info = mock_info("new_admin_2", &[]);
        let msg = ExecuteMsg::CancelAdminTransfer {};

        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        // admin can cancel admin transfer
        let info = mock_info("new_admin", &[]);
        let msg = ExecuteMsg::CancelAdminTransfer {};

        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let (admin, candidate) = query_admin_and_candidate(deps.as_ref());
        assert_eq!(admin, Some("new_admin".to_string()));
        assert_eq!(candidate, None);

        // only admin can revoke admin
        let info = mock_info("new_admin_2", &[]);
        let msg = ExecuteMsg::RevokeAdmin {};

        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        // admin can revoke admin
        let info = mock_info("new_admin", &[]);
        let msg = ExecuteMsg::RevokeAdmin {};

        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let (admin, candidate) = query_admin_and_candidate(deps.as_ref());
        assert_eq!(admin, None);
        assert_eq!(candidate, None);
    }

    fn query_admin_and_candidate(deps: Deps) -> (Option<String>, Option<String>) {
        let AdminResponse { admin } =
            from_json(query(deps, mock_env(), QueryMsg::Admin {}).unwrap()).unwrap();

        let AdminCandidateResponse { candidate } =
            from_json(query(deps, mock_env(), QueryMsg::AdminCandidate {}).unwrap()).unwrap();

        (admin, candidate)
    }

    // TODO: test set tracked denoms
    // test admin can set tracked denoms
    // need to mock stargate query

    #[test]
    fn test_set_and_remove_tracked_denoms() {
        let mut deps = mock_dependencies_with_stargate_querier(
            &[("creator", &[Coin::new(100000, "uusdc")])],
            arithmetic_twap_to_now_query_handler(Box::new(|req| {
                let base_asset = req.base_asset.as_str();
                let quote_asset = req.quote_asset.as_str();

                let arithmetic_twap = match (base_asset, quote_asset) {
                    ("uosmo", "uusdc") => "1.5",
                    ("uatom", "uusdc") => "2.5",
                    ("uion", "uusdc") => "3.5",
                    ("ustars", "uusdc") => "4.5",
                    ("uosmo", "uatom") => "0.5",
                    _ => return ContractResult::Err("Price not found".to_string()),
                }
                .to_string();

                ContractResult::Ok(ArithmeticTwapToNowResponse { arithmetic_twap })
            })),
        );

        let msg = InstantiateMsg {
            price_resolution_config: PriceResolutionConfig {
                quote_denom: "uusdc".to_string(),
                staleness_threshold: Uint64::from(3_600_000_000u64),
                twap_duration: Uint64::from(3_600_000_000u64),
            },
            tracked_denoms: vec![],
            admin: Some("admin".to_string()),
        };

        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // query tracked denoms
        let TrackedDenomsResponse { tracked_denoms } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TrackedDenoms {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(tracked_denoms, vec![]);

        // set tracked denoms
        let tracked_denoms = vec![
            TrackedDenom {
                denom: "uatom".to_string(),
                swap_routes: vec![SwapAmountInRoute {
                    pool_id: 2,
                    token_out_denom: "uusdc".to_string(),
                }],
            },
            TrackedDenom {
                denom: "uion".to_string(),
                swap_routes: vec![SwapAmountInRoute {
                    pool_id: 3,
                    token_out_denom: "uusdc".to_string(),
                }],
            },
            TrackedDenom {
                denom: "uosmo".to_string(),
                swap_routes: vec![SwapAmountInRoute {
                    pool_id: 1,
                    token_out_denom: "uusdc".to_string(),
                }],
            },
            TrackedDenom {
                denom: "ustars".to_string(),
                swap_routes: vec![SwapAmountInRoute {
                    pool_id: 4,
                    token_out_denom: "uusdc".to_string(),
                }],
            },
        ];
        let info = mock_info("non_admin", &[]);
        let msg = ExecuteMsg::SetTrackedDenoms {
            tracked_denoms: tracked_denoms.clone(),
        };

        let res = execute(deps.as_mut(), mock_env(), info, msg);

        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        let info = mock_info("admin", &[]);
        let msg = ExecuteMsg::SetTrackedDenoms {
            tracked_denoms: tracked_denoms.clone(),
        };

        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        // query tracked denoms
        let TrackedDenomsResponse {
            tracked_denoms: tracked_denoms_response,
        } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TrackedDenoms {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(tracked_denoms_response, tracked_denoms);

        // remove tracked denoms
        let info = mock_info("non_admin", &[]);
        let msg = ExecuteMsg::RemoveTrackedDenoms {
            target: DenomRemovalTarget::Partial(vec!["uatom".to_string(), "ustars".to_string()]),
        };

        let res = execute(deps.as_mut(), mock_env(), info, msg.clone());

        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        let info = mock_info("admin", &[]);
        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        // query tracked denoms
        let TrackedDenomsResponse {
            tracked_denoms: tracked_denoms_response,
        } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TrackedDenoms {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap(),
        )
        .unwrap();

        let expected = vec![
            TrackedDenom {
                denom: "uion".to_string(),
                swap_routes: vec![SwapAmountInRoute {
                    pool_id: 3,
                    token_out_denom: "uusdc".to_string(),
                }],
            },
            TrackedDenom {
                denom: "uosmo".to_string(),
                swap_routes: vec![SwapAmountInRoute {
                    pool_id: 1,
                    token_out_denom: "uusdc".to_string(),
                }],
            },
        ];

        assert_eq!(tracked_denoms_response, expected);

        // set tracked denoms with replacement
        let tracked_denoms = vec![
            TrackedDenom {
                denom: "uosmo".to_string(),
                swap_routes: vec![
                    SwapAmountInRoute {
                        pool_id: 10,
                        token_out_denom: "uatom".to_string(),
                    },
                    SwapAmountInRoute {
                        pool_id: 2,
                        token_out_denom: "uusdc".to_string(),
                    },
                ],
            },
            TrackedDenom {
                denom: "uatom".to_string(),
                swap_routes: vec![SwapAmountInRoute {
                    pool_id: 2,
                    token_out_denom: "uusdc".to_string(),
                }],
            },
            TrackedDenom {
                denom: "ustars".to_string(),
                swap_routes: vec![SwapAmountInRoute {
                    pool_id: 4,
                    token_out_denom: "uusdc".to_string(),
                }],
            },
        ];

        let info = mock_info("admin", &[]);

        let msg = ExecuteMsg::SetTrackedDenoms {
            tracked_denoms: tracked_denoms.clone(),
        };

        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        // query tracked denoms
        let TrackedDenomsResponse {
            tracked_denoms: tracked_denoms_response,
        } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TrackedDenoms {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap(),
        )
        .unwrap();

        let expected = vec![
            TrackedDenom {
                denom: "uatom".to_string(),
                swap_routes: vec![SwapAmountInRoute {
                    pool_id: 2,
                    token_out_denom: "uusdc".to_string(),
                }],
            },
            TrackedDenom {
                denom: "uion".to_string(),
                swap_routes: vec![SwapAmountInRoute {
                    pool_id: 3,
                    token_out_denom: "uusdc".to_string(),
                }],
            },
            TrackedDenom {
                denom: "uosmo".to_string(),
                swap_routes: vec![
                    SwapAmountInRoute {
                        pool_id: 10,
                        token_out_denom: "uatom".to_string(),
                    },
                    SwapAmountInRoute {
                        pool_id: 2,
                        token_out_denom: "uusdc".to_string(),
                    },
                ],
            },
            TrackedDenom {
                denom: "ustars".to_string(),
                swap_routes: vec![SwapAmountInRoute {
                    pool_id: 4,
                    token_out_denom: "uusdc".to_string(),
                }],
            },
        ];
        assert_eq!(tracked_denoms_response, expected);

        let TrackedDenomsResponse {
            tracked_denoms: tracked_denoms_response,
        } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TrackedDenoms {
                    start_after: None,
                    limit: Some(2),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(tracked_denoms_response, expected[..2]);

        let TrackedDenomsResponse {
            tracked_denoms: tracked_denoms_response,
        } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TrackedDenoms {
                    start_after: Some("uion".to_string()),
                    limit: Some(1),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(tracked_denoms_response, expected[2..3]);

        let TrackedDenomsResponse {
            tracked_denoms: tracked_denoms_response,
        } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TrackedDenoms {
                    start_after: Some("uion".to_string()),
                    limit: Some(2),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(tracked_denoms_response, expected[2..]);

        let TrackedDenomsResponse {
            tracked_denoms: tracked_denoms_response,
        } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TrackedDenoms {
                    start_after: Some("uion".to_string()),
                    limit: None,
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(tracked_denoms_response, expected[2..]);

        // remove all
        let info = mock_info("admin", &[]);

        let msg = ExecuteMsg::RemoveTrackedDenoms {
            target: DenomRemovalTarget::All,
        };

        execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        // query tracked denoms
        let TrackedDenomsResponse {
            tracked_denoms: tracked_denoms_response,
        } = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TrackedDenoms {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(tracked_denoms_response, vec![]);
    }
}

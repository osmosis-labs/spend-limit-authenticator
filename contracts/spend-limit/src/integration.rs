// Ignore integration tests for code coverage since there will be problems with dynamic linking libosmosistesttube
// and also, tarpaulin will not be able read coverage out of wasm binary anyway
#![cfg(all(test, not(tarpaulin)))]

use cosmwasm_std::{Coin, Timestamp, Uint128};

use osmosis_std::types::osmosis::{
    authenticator::{self, MsgRemoveAuthenticator, MsgRemoveAuthenticatorResponse},
    gamm::v1beta1::MsgSwapExactAmountInResponse,
    poolmanager::v1beta1::{MsgSwapExactAmountIn, SwapAmountInRoute},
};
use osmosis_test_tube::{
    cosmrs::proto::tendermint::v0_37::abci::ResponseDeliverTx,
    osmosis_std::types::cosmos::bank::v1beta1::MsgSend, Account, Gamm, Module, OsmosisTestApp,
    Runner, RunnerExecuteResult, SigningAccount, Wasm,
};
use time::{Duration, OffsetDateTime};

use crate::{
    assert_substring,
    msg::{InstantiateMsg, QueryMsg, SpendingResponse, SpendingsByAccountResponse, TrackedDenom},
    price::{PriceError, PriceResolutionConfig},
    spend_limit::{Period, SpendLimitError, SpendLimitParams, Spending, TimeLimit},
    test_helper::authenticator_setup::{
        add_1ct_session_authenticator, add_all_of_sig_ver_spend_limit_authenticator,
        add_spend_limit_authenticator, spend_limit_instantiate, spend_limit_store_code,
    },
    ContractError,
};

const UUSDC: &str = "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4";
const UATOM: &str = "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2";

#[test]
fn test_no_conversion() {
    let app = OsmosisTestApp::new();
    set_maximum_unauthenticated_gas(&app, MAXIMUM_UNAUTHENTICATED_GAS);
    let accs = app
        .init_accounts(&[Coin::new(1_000_000_000_000_000, "uosmo")], 2)
        .unwrap();

    let wasm = Wasm::new(&app);

    // Store code and initialize spend limit contract
    let code_id = spend_limit_store_code(&wasm, &accs[0]);
    let contract_addr = spend_limit_instantiate(
        &wasm,
        code_id,
        &InstantiateMsg {
            price_resolution_config: PriceResolutionConfig {
                quote_denom: "uosmo".to_string(),
                staleness_threshold: 3_600_000_000_000u64.into(), // 1h
                twap_duration: 3_600_000_000_000u64.into(),       // 1h
            },
            tracked_denoms: vec![],
        },
        &accs[0],
    );

    // Add spend limit authenticator
    let spend_limit_auth_id = add_spend_limit_authenticator(
        &app,
        &accs[0],
        &contract_addr,
        &SpendLimitParams {
            limit: Uint128::new(1_000_000),
            reset_period: Period::Day,
            time_limit: None,
        },
    );

    // spend to the limit
    bank_send(
        &app,
        &accs[0],
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(1_000_000, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    // spend some more
    let res = bank_send(
        &app,
        &accs[0],
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(1, "uosmo")],
        spend_limit_auth_id,
    );

    assert_substring!(
        res.as_ref().unwrap_err().to_string(),
        SpendLimitError::overspend(0, 1).to_string()
    );

    let prev_ts = app.get_block_time_seconds() as i64;
    let prev_dt = OffsetDateTime::from_unix_timestamp(prev_ts).unwrap();
    let next_dt = (prev_dt + Duration::days(1)).unix_timestamp();
    let diff = next_dt - prev_ts;

    app.increase_time(diff as u64);

    bank_send(
        &app,
        &accs[0],
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(500_000, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    bank_send(
        &app,
        &accs[0],
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(499_999, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    let err = bank_send(
        &app,
        &accs[0],
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(2, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap_err();

    assert_substring!(
        err.to_string(),
        SpendLimitError::overspend(1, 2).to_string()
    );
}

#[test]
fn test_with_conversion() {
    let app = OsmosisTestApp::new();
    set_maximum_unauthenticated_gas(&app, MAXIMUM_UNAUTHENTICATED_GAS);

    let accs = app
        .init_accounts(
            &[
                Coin::new(1_000_000_000_000_000, "uosmo"),
                Coin::new(1_000_000_000_000_000, "uion"),
                Coin::new(1_000_000_000_000_000, UUSDC),
                Coin::new(1_000_000_000_000_000, UATOM),
            ],
            2,
        )
        .unwrap();

    let gamm = Gamm::new(&app);

    // 1:1.5
    let osmo_usdc_pool_id = gamm
        .create_basic_pool(
            &[
                Coin::new(1_000_000_000, "uosmo"),
                Coin::new(1_500_000_000, UUSDC),
            ],
            &accs[0],
        )
        .unwrap()
        .data
        .pool_id;

    // 4:1
    let ion_osmo_pool_id = gamm
        .create_basic_pool(
            &[Coin::new(4_000_000, "uion"), Coin::new(1_000_000, "uosmo")],
            &accs[0],
        )
        .unwrap()
        .data
        .pool_id;

    // 4:1
    let ion_atom_pool_id = gamm
        .create_basic_pool(
            &[Coin::new(4_000_000, "uion"), Coin::new(1_000_000, UATOM)],
            &accs[0],
        )
        .unwrap()
        .data
        .pool_id;

    // 1:1
    let atom_osmo_pool_id = gamm
        .create_basic_pool(
            &[Coin::new(1_000_000, UATOM), Coin::new(1_000_000, "uosmo")],
            &accs[0],
        )
        .unwrap()
        .data
        .pool_id;

    // increase time by 1h
    app.increase_time(3_600u64);

    let wasm = Wasm::new(&app);

    // Store code and initialize spend limit contract
    let code_id = spend_limit_store_code(&wasm, &accs[0]);

    // try instantiate with incorrect routes
    let now = app.get_block_timestamp();
    let start_time = now.minus_nanos(3_600_000_000_000u64);
    let err = wasm
        .instantiate(
            code_id,
            &InstantiateMsg {
                price_resolution_config: PriceResolutionConfig {
                    quote_denom: UUSDC.to_string(),
                    staleness_threshold: 3_600_000_000_000u64.into(), // 1h
                    twap_duration: 3_600_000_000_000u64.into(),       // 1h
                },
                tracked_denoms: vec![
                    TrackedDenom {
                        denom: "uosmo".to_string(),
                        swap_routes: vec![SwapAmountInRoute {
                            pool_id: osmo_usdc_pool_id,
                            token_out_denom: UUSDC.to_string(),
                        }],
                    },
                    // incorrect
                    TrackedDenom {
                        denom: "uion".to_string(),
                        swap_routes: vec![
                            SwapAmountInRoute {
                                pool_id: ion_atom_pool_id,
                                token_out_denom: UATOM.to_string(),
                            },
                            SwapAmountInRoute {
                                pool_id: osmo_usdc_pool_id,
                                token_out_denom: UUSDC.to_string(),
                            },
                        ],
                    },
                ],
            },
            None,
            Some("spend_limit_authenticator"),
            &[],
            &accs[0],
        )
        .unwrap_err();

    assert_substring!(
        err.to_string(),
        PriceError::twap_query_error(osmo_usdc_pool_id, UATOM, UUSDC, start_time).to_string()
    );

    let contract_addr = spend_limit_instantiate(
        &wasm,
        code_id,
        &InstantiateMsg {
            price_resolution_config: PriceResolutionConfig {
                quote_denom: UUSDC.to_string(),
                staleness_threshold: 3_600_000_000_000u64.into(), // 1h
                twap_duration: 3_600_000_000_000u64.into(),       // 1h
            },
            tracked_denoms: vec![
                TrackedDenom {
                    denom: "uosmo".to_string(),
                    swap_routes: vec![SwapAmountInRoute {
                        pool_id: osmo_usdc_pool_id,
                        token_out_denom: UUSDC.to_string(),
                    }],
                },
                TrackedDenom {
                    denom: "uion".to_string(),
                    swap_routes: vec![
                        SwapAmountInRoute {
                            pool_id: ion_osmo_pool_id,
                            token_out_denom: "uosmo".to_string(),
                        },
                        SwapAmountInRoute {
                            pool_id: osmo_usdc_pool_id,
                            token_out_denom: UUSDC.to_string(),
                        },
                    ],
                },
                TrackedDenom {
                    denom: UATOM.to_string(),
                    swap_routes: vec![
                        SwapAmountInRoute {
                            pool_id: atom_osmo_pool_id,
                            token_out_denom: "uosmo".to_string(),
                        },
                        SwapAmountInRoute {
                            pool_id: osmo_usdc_pool_id,
                            token_out_denom: UUSDC.to_string(),
                        },
                    ],
                },
            ],
        },
        &accs[0],
    );

    // Add spend limit authenticator
    let spend_limit_auth_id = add_spend_limit_authenticator(
        &app,
        &accs[0],
        &contract_addr,
        &SpendLimitParams {
            limit: Uint128::new(1_000_000),
            reset_period: Period::Day,
            time_limit: None,
        },
    );

    // spend to the limit
    bank_send(
        &app,
        &accs[0],
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(666_666, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    // spend some more
    let res = bank_send(
        &app,
        &accs[0],
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(2, UUSDC)],
        spend_limit_auth_id,
    );

    assert_substring!(
        res.as_ref().unwrap_err().to_string(),
        SpendLimitError::overspend(1, 2).to_string()
    );

    let prev_ts = app.get_block_time_seconds() as i64;
    let prev_dt = OffsetDateTime::from_unix_timestamp(prev_ts).unwrap();
    let next_dt = (prev_dt + Duration::days(1)).unix_timestamp();
    let diff = next_dt - prev_ts;

    app.increase_time(diff as u64);

    bank_send(
        &app,
        &accs[0],
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(500_000, UUSDC)],
        spend_limit_auth_id,
    )
    .unwrap();

    bank_send(
        &app,
        &accs[0],
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(499_999, UUSDC)],
        spend_limit_auth_id,
    )
    .unwrap();

    let err = bank_send(
        &app,
        &accs[0],
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(6, "uion")],
        spend_limit_auth_id,
    )
    .unwrap_err();

    assert_substring!(
        err.to_string(),
        SpendLimitError::overspend(1, 2).to_string()
    );
}

#[test]
fn test_setup_and_teardown() {
    let app = OsmosisTestApp::new();
    set_maximum_unauthenticated_gas(&app, MAXIMUM_UNAUTHENTICATED_GAS);

    let accs = app
        .init_accounts(&[Coin::new(1_000_000_000_000_000, "uosmo")], 2)
        .unwrap();

    let wasm = Wasm::new(&app);

    // Store code and initialize spend limit contract
    let code_id = spend_limit_store_code(&wasm, &accs[0]);
    let contract_addr = spend_limit_instantiate(
        &wasm,
        code_id,
        &InstantiateMsg {
            price_resolution_config: PriceResolutionConfig {
                quote_denom: "uosmo".to_string(),
                staleness_threshold: 3_600_000_000_000u64.into(), // 1h
                twap_duration: 3_600_000_000_000u64.into(),       // 1h
            },
            tracked_denoms: vec![],
        },
        &accs[0],
    );

    // Add spend limit authenticator
    add_all_of_sig_ver_spend_limit_authenticator(
        &app,
        &accs[0],
        &contract_addr,
        &SpendLimitParams {
            limit: Uint128::new(1_000_000),
            reset_period: Period::Day,
            time_limit: None,
        },
    );

    add_all_of_sig_ver_spend_limit_authenticator(
        &app,
        &accs[0],
        &contract_addr,
        &SpendLimitParams {
            limit: Uint128::new(999_999),
            reset_period: Period::Day,
            time_limit: None,
        },
    );

    add_all_of_sig_ver_spend_limit_authenticator(
        &app,
        &accs[1],
        &contract_addr,
        &SpendLimitParams {
            limit: Uint128::new(100_000),
            reset_period: Period::Day,
            time_limit: None,
        },
    );

    let SpendingsByAccountResponse { spendings } = wasm
        .query(
            &contract_addr,
            &QueryMsg::SpendingsByAccount {
                account: accs[0].address(),
            },
        )
        .unwrap();

    assert_eq!(
        spendings,
        vec![
            ("1.1".to_string(), Spending::default()),
            ("2.1".to_string(), Spending::default()),
        ]
    );

    let SpendingsByAccountResponse { spendings } = wasm
        .query(
            &contract_addr,
            &QueryMsg::SpendingsByAccount {
                account: accs[1].address(),
            },
        )
        .unwrap();

    assert_eq!(spendings, vec![("3.1".to_string(), Spending::default())]);

    // Remove spend limit authenticator
    app.execute::<_, MsgRemoveAuthenticatorResponse>(
        MsgRemoveAuthenticator {
            sender: accs[0].address(),
            id: 1,
        },
        MsgRemoveAuthenticator::TYPE_URL,
        &accs[0],
    )
    .unwrap();

    let SpendingsByAccountResponse { spendings } = wasm
        .query(
            &contract_addr,
            &QueryMsg::SpendingsByAccount {
                account: accs[0].address(),
            },
        )
        .unwrap();

    assert_eq!(spendings, vec![("2.1".to_string(), Spending::default())]);
}

#[test]
fn test_1_click_trading() {
    let app = OsmosisTestApp::new();

    set_maximum_unauthenticated_gas(&app, MAXIMUM_UNAUTHENTICATED_GAS);

    let acc = app
        .init_account(&[
            Coin::new(1_000_000_000_000_000, "uosmo"),
            Coin::new(1_000_000_000_000_000, "uion"),
            Coin::new(1_000_000_000_000_000, UUSDC),
            Coin::new(1_000_000_000_000_000, UATOM),
        ])
        .unwrap();

    let empty_accs = app.init_accounts(&[], 3).unwrap();

    let gamm = Gamm::new(&app);

    // 1:1.5
    let osmo_usdc_pool_id = gamm
        .create_basic_pool(
            &[
                Coin::new(1_000_000_000, "uosmo"),
                Coin::new(1_500_000_000, UUSDC),
            ],
            &acc,
        )
        .unwrap()
        .data
        .pool_id;

    // 4:1
    let ion_osmo_pool_id = gamm
        .create_basic_pool(
            &[Coin::new(4_000_000, "uion"), Coin::new(1_000_000, "uosmo")],
            &acc,
        )
        .unwrap()
        .data
        .pool_id;

    // 4:1
    let ion_atom_pool_id = gamm
        .create_basic_pool(
            &[Coin::new(4_000_000, "uion"), Coin::new(1_000_000, UATOM)],
            &acc,
        )
        .unwrap()
        .data
        .pool_id;

    // 1:1
    let atom_osmo_pool_id = gamm
        .create_basic_pool(
            &[Coin::new(1_000_000, UATOM), Coin::new(1_000_000, "uosmo")],
            &acc,
        )
        .unwrap()
        .data
        .pool_id;

    // increase time by 1h for twap to warm up
    app.increase_time(3_600u64);

    let wasm = Wasm::new(&app);

    // Store code and initialize spend limit contract
    let code_id = spend_limit_store_code(&wasm, &acc);

    // try instantiate with incorrect routes
    let contract_addr = spend_limit_instantiate(
        &wasm,
        code_id,
        &InstantiateMsg {
            price_resolution_config: PriceResolutionConfig {
                quote_denom: UUSDC.to_string(),
                staleness_threshold: 3_600_000_000_000u64.into(), // 1h
                twap_duration: 3_600_000_000_000u64.into(),       // 1h
            },
            tracked_denoms: vec![
                TrackedDenom {
                    denom: "uosmo".to_string(),
                    swap_routes: vec![SwapAmountInRoute {
                        pool_id: osmo_usdc_pool_id,
                        token_out_denom: UUSDC.to_string(),
                    }],
                },
                TrackedDenom {
                    denom: "uion".to_string(),
                    swap_routes: vec![
                        SwapAmountInRoute {
                            pool_id: ion_osmo_pool_id,
                            token_out_denom: "uosmo".to_string(),
                        },
                        SwapAmountInRoute {
                            pool_id: osmo_usdc_pool_id,
                            token_out_denom: UUSDC.to_string(),
                        },
                    ],
                },
                TrackedDenom {
                    denom: UATOM.to_string(),
                    swap_routes: vec![
                        SwapAmountInRoute {
                            pool_id: atom_osmo_pool_id,
                            token_out_denom: "uosmo".to_string(),
                        },
                        SwapAmountInRoute {
                            pool_id: osmo_usdc_pool_id,
                            token_out_denom: UUSDC.to_string(),
                        },
                    ],
                },
            ],
        },
        &acc,
    );

    let account_owner = &acc;
    let one_click_trading_session_signer_1 = &empty_accs[0];
    let one_click_trading_session_pubkey_1 =
        one_click_trading_session_signer_1.public_key().to_bytes();

    let session_1_end = Timestamp::from_seconds(app.get_block_time_seconds() as u64).plus_days(3);
    let one_click_trading_auth_id_1 = add_1ct_session_authenticator(
        &app,
        &account_owner,
        &one_click_trading_session_pubkey_1,
        &contract_addr,
        &SpendLimitParams {
            limit: Uint128::new(5_000_000_000),
            reset_period: Period::Day,
            time_limit: Some(TimeLimit {
                start: None,
                end: session_1_end,
            }),
        },
    );

    // only `MsgSwapExactAmountIn` and `MsgSplitRouteSwapExactAmountIn` are allowed
    let err = bank_send(
        &app,
        &account_owner,
        &one_click_trading_session_signer_1,
        &account_owner.address(),
        vec![Coin::new(1, "uosmo")],
        one_click_trading_auth_id_1,
    )
    .unwrap_err();

    assert_substring!(
        err.to_string(),
        "all sub-authenticators failed to authenticate".to_string()
    );

    // wrong signer
    let wrong_signer = &empty_accs[1];
    let err = one_click_swap_exact_amount_in(
        &app,
        &account_owner,
        &wrong_signer,
        vec![SwapAmountInRoute {
            pool_id: osmo_usdc_pool_id,
            token_out_denom: "uosmo".to_string(),
        }],
        Coin::new(100, UUSDC),
        1,
        one_click_trading_auth_id_1,
    )
    .unwrap_err();
    assert_substring!(err.to_string(), "signature verification failed".to_string());

    one_click_swap_exact_amount_in(
        &app,
        &account_owner,
        &one_click_trading_session_signer_1,
        vec![SwapAmountInRoute {
            pool_id: osmo_usdc_pool_id,
            token_out_denom: "uosmo".to_string(),
        }],
        Coin::new(100, UUSDC),
        1,
        one_click_trading_auth_id_1,
    )
    .unwrap();

    // query spendings
    let SpendingResponse { spending } = wasm
        .query(
            &contract_addr,
            &QueryMsg::Spending {
                account: account_owner.address(),
                authenticator_id: format!("{one_click_trading_auth_id_1}.1"),
            },
        )
        .unwrap();

    assert_eq!(spending.value_spent_in_period.u128(), 100);

    // swap ion to atom with overspend
    let err = one_click_swap_exact_amount_in(
        &app,
        &account_owner,
        &one_click_trading_session_signer_1,
        vec![SwapAmountInRoute {
            pool_id: ion_atom_pool_id,
            token_out_denom: UATOM.to_string(),
        }],
        // remaining quota in uion = ((5_000_000_000 - 100) / 1.5) * 4 ~= 1_333_333_067
        // +((1 / 1.5) * 4) ~= 3 to make it overspend
        Coin::new(13_333_333_070, "uion"),
        1,
        one_click_trading_auth_id_1,
    )
    .unwrap_err();

    assert_substring!(
        err.to_string(),
        SpendLimitError::overspend(4999999900, 4999999901).to_string()
    );

    // create another session
    let one_click_trading_session_signer_2 = &empty_accs[1];
    let one_click_trading_session_pubkey_2 =
        one_click_trading_session_signer_2.public_key().to_bytes();

    let one_click_trading_auth_id_2 = add_1ct_session_authenticator(
        &app,
        &account_owner,
        &one_click_trading_session_pubkey_2,
        &contract_addr,
        &SpendLimitParams {
            limit: Uint128::new(10_000_000_000),
            reset_period: Period::Month,
            time_limit: Some(TimeLimit {
                start: None,
                end: Timestamp::from_seconds(app.get_block_time_seconds() as u64).plus_hours(3),
            }),
        },
    );

    // signer 1 should not be able to sign for session 2
    let err = one_click_swap_exact_amount_in(
        &app,
        &account_owner,
        &one_click_trading_session_signer_1,
        vec![SwapAmountInRoute {
            pool_id: osmo_usdc_pool_id,
            token_out_denom: "uosmo".to_string(),
        }],
        Coin::new(100, UUSDC),
        1,
        one_click_trading_auth_id_2,
    )
    .unwrap_err();

    assert_substring!(err.to_string(), "signature verification failed".to_string());

    // one click swap the failed one with new session should pass
    one_click_swap_exact_amount_in(
        &app,
        &account_owner,
        &one_click_trading_session_signer_2,
        vec![SwapAmountInRoute {
            pool_id: ion_atom_pool_id,
            token_out_denom: UATOM.to_string(),
        }],
        // remaining quota in uion = ((5_000_000_000 - 100) / 1.5) * 4 ~= 1_333_333_067
        // +((1 / 1.5) * 4) ~= 3 to make it overspend
        Coin::new(13_333_333_070, "uion"),
        1,
        one_click_trading_auth_id_2,
    )
    .unwrap();

    // query spendings for session 2
    let SpendingResponse { spending } = wasm
        .query(
            &contract_addr,
            &QueryMsg::Spending {
                account: account_owner.address(),
                authenticator_id: format!("{one_click_trading_auth_id_2}.1"),
            },
        )
        .unwrap();

    assert_eq!(spending.value_spent_in_period.u128(), 4_999_999_901);

    // query spending for session 1
    let SpendingResponse { spending } = wasm
        .query(
            &contract_addr,
            &QueryMsg::Spending {
                account: account_owner.address(),
                authenticator_id: format!("{one_click_trading_auth_id_1}.1"),
            },
        )
        .unwrap();

    assert_eq!(spending.value_spent_in_period.u128(), 100);

    // increases time for 2 days
    app.increase_time(24 * 60 * 60 * 2);

    // TODO: make this test pass: query spending for session 1
    // let SpendingResponse { spending } = wasm
    //     .query(
    //         &contract_addr,
    //         &QueryMsg::Spending {
    //             account: account_owner.address(),
    //             authenticator_id: format!("{one_click_trading_auth_id_1}.1"),
    //         },
    //     )
    //     .unwrap();

    // assert_eq!(spending.value_spent_in_period.u128(), 0);

    // spend almost all of the quota
    one_click_swap_exact_amount_in(
        &app,
        &account_owner,
        &one_click_trading_session_signer_1,
        vec![SwapAmountInRoute {
            pool_id: osmo_usdc_pool_id,
            token_out_denom: "uosmo".to_string(),
        }],
        Coin::new(4_999_999_999, UUSDC),
        1,
        one_click_trading_auth_id_1,
    )
    .unwrap();

    // query spending for session 1
    let SpendingResponse { spending } = wasm
        .query(
            &contract_addr,
            &QueryMsg::Spending {
                account: account_owner.address(),
                authenticator_id: format!("{one_click_trading_auth_id_1}.1"),
            },
        )
        .unwrap();

    assert_eq!(spending.value_spent_in_period.u128(), 4_999_999_999);

    // increase time for 1 day, which the time limit for session 1 is over (3 days)
    app.increase_time(24 * 60 * 60 * 1);

    // try spend the last bit should fail due to time limit
    let err = one_click_swap_exact_amount_in(
        &app,
        &account_owner,
        &one_click_trading_session_signer_1,
        vec![SwapAmountInRoute {
            pool_id: osmo_usdc_pool_id,
            token_out_denom: "uosmo".to_string(),
        }],
        Coin::new(1, UUSDC),
        1,
        one_click_trading_auth_id_1,
    )
    .unwrap_err();

    let current = Timestamp::from_nanos(app.get_block_time_nanos() as u64);
    assert_substring!(
        err.to_string(),
        ContractError::NotWithinTimeLimit {
            current,
            start: None,
            end: session_1_end
        }
        .to_string()
    );
}

fn bank_send(
    app: &OsmosisTestApp,
    account: &SigningAccount,
    signer: &SigningAccount,
    to_address: &str,
    amount: Vec<Coin>,
    authenticator_id: u64,
) -> RunnerExecuteResult<ResponseDeliverTx> {
    let amount: Vec<osmosis_test_tube::osmosis_std::types::cosmos::base::v1beta1::Coin> =
        amount.into_iter().map(Into::into).collect();

    app.execute_with_selected_authenticators(
        vec![MsgSend {
            from_address: account.address(),
            to_address: to_address.to_string(),
            amount,
        }
        .to_any()
        .into()],
        account,
        signer,
        &[authenticator_id],
    )?
    .try_into()
}

fn one_click_swap_exact_amount_in(
    app: &OsmosisTestApp,
    from: &SigningAccount,
    session_signer: &SigningAccount,
    routes: Vec<SwapAmountInRoute>,
    token_in: Coin,
    token_out_min_amount: u128,
    authenticator_id: u64,
) -> RunnerExecuteResult<MsgSwapExactAmountInResponse> {
    app.execute_with_selected_authenticators(
        vec![MsgSwapExactAmountIn {
            sender: from.address(),
            routes,
            token_in: Some(token_in.into()),
            token_out_min_amount: token_out_min_amount.to_string(),
        }
        .to_any()
        .into()],
        from,
        session_signer,
        &[authenticator_id],
    )?
    .try_into()
}

const MAXIMUM_UNAUTHENTICATED_GAS: u64 = 120_000;
fn set_maximum_unauthenticated_gas(app: &OsmosisTestApp, maximum_unauthenticated_gas: u64) {
    app.set_param_set(
        "authenticator",
        authenticator::Params {
            maximum_unauthenticated_gas,
            are_smart_accounts_active: true,
        }
        .to_any(),
    )
    .unwrap();
}

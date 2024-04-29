// Ignore integration tests for code coverage since there will be problems with dynamic linking libosmosistesttube
// and also, tarpaulin will not be able to read coverage out of wasm binary anyway
#![cfg(all(test, not(tarpaulin)))]

use cosmwasm_std::{Coin, Timestamp, Uint128};
use osmosis_std::types::osmosis::poolmanager::v1beta1::{
    EstimateSwapExactAmountInRequest, EstimateSwapExactAmountInResponse,
};
use osmosis_std::types::osmosis::smartaccount;
use osmosis_std::types::osmosis::{
    gamm::v1beta1::MsgSwapExactAmountInResponse,
    poolmanager::v1beta1::{MsgSwapExactAmountIn, SwapAmountInRoute},
    smartaccount::v1beta1::{MsgRemoveAuthenticator, MsgRemoveAuthenticatorResponse},
};
use osmosis_test_tube::osmosis_std::types::cosmos::bank::v1beta1::QueryBalanceRequest;
use osmosis_test_tube::{
    cosmrs::proto::tendermint::v0_37::abci::ResponseDeliverTx,
    osmosis_std::types::cosmos::bank::v1beta1::MsgSend, Account, Bank, FeeSetting, Gamm, Module,
    OsmosisTestApp, Runner, RunnerExecuteResult, RunnerResult, SigningAccount, Wasm,
};
use time::{Duration, OffsetDateTime};

use crate::ContractError;
use crate::{
    assert_substring,
    msg::{InstantiateMsg, QueryMsg, SpendingResponse, SpendingsByAccountResponse, TrackedDenom},
    period::Period,
    price::{PriceError, PriceResolutionConfig},
    spend_limit::{SpendLimitError, SpendLimitParams, Spending, TimeLimit},
    test_helper::authenticator_setup::{
        add_1ct_session_authenticator, add_all_of_sig_ver_spend_limit_authenticator,
        add_spend_limit_authenticator, spend_limit_instantiate, spend_limit_store_code,
    },
};

const UUSDC: &str = "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4";
const UATOM: &str = "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2";

#[test]
fn test_no_conversion() {
    let app = OsmosisTestApp::new();
    set_maximum_unauthenticated_gas(&app, MAXIMUM_UNAUTHENTICATED_GAS);
    let acc_1 = app
        .init_account(&[Coin::new(1_000_000_000_000_000, "uosmo")])
        .unwrap();

    let acc_2 = app
        .init_account(&[Coin::new(1_000_000_000_000_000, "uosmo")])
        .unwrap();

    let wasm = Wasm::new(&app);

    // Store code and initialize spend limit contract
    let code_id = spend_limit_store_code(&wasm, &acc_1);
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
            admin: None,
        },
        &acc_1,
    );

    let spend_limit_querier = SpendLimitQuerier::new(&app, contract_addr.to_string());

    // Add spend limit authenticator
    let spend_limit_auth_id = add_spend_limit_authenticator(
        &app,
        &acc_1,
        &contract_addr,
        &SpendLimitParams {
            limit: Uint128::new(1_500_000),
            reset_period: Period::Day,
            time_limit: None,
        },
    );

    let acc_1_custom_fee = acc_1.with_fee_setting(FeeSetting::Custom {
        amount: Coin::new(500_000, "uosmo"),
        gas_limit: 1_000_000,
    });

    // spend to the limit
    bank_send(
        &app,
        &acc_1_custom_fee,
        &acc_1_custom_fee,
        &acc_2.address(),
        vec![Coin::new(1_000_000, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    let timestamp = Timestamp::from_nanos(app.get_block_time_nanos() as u64);

    assert_eq!(
        spend_limit_querier
            .query_spendings_by_account(acc_1_custom_fee.address())
            .unwrap(),
        vec![(
            "1".to_string(),
            Spending {
                value_spent_in_period: Uint128::new(1_500_000),
                last_spent_at: timestamp
            }
        )]
    );

    // spend some more
    let acc_1_custom_fee = acc_1_custom_fee.with_fee_setting(FeeSetting::Custom {
        amount: Coin::new(2500, "uosmo"),
        gas_limit: 1_000_000,
    });
    let res = bank_send(
        &app,
        &acc_1_custom_fee,
        &acc_1_custom_fee,
        &acc_2.address(),
        vec![Coin::new(1, "uosmo")],
        spend_limit_auth_id,
    );
    assert_substring!(
        res.as_ref().unwrap_err().to_string(),
        SpendLimitError::overspend(1_500_000, 1_502_500).to_string()
    );

    let prev_ts = app.get_block_time_seconds();
    let prev_dt = OffsetDateTime::from_unix_timestamp(prev_ts).unwrap();
    let next_dt = (prev_dt + Duration::days(1)).unix_timestamp();
    let diff = next_dt - prev_ts;

    app.increase_time(diff as u64);

    bank_send(
        &app,
        &acc_1_custom_fee,
        &acc_1_custom_fee,
        &acc_2.address(),
        vec![Coin::new(1_400_000, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    bank_send(
        &app,
        &acc_1_custom_fee,
        &acc_1_custom_fee,
        &acc_2.address(),
        vec![Coin::new(92_500, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    let err = bank_send(
        &app,
        &acc_1_custom_fee,
        &acc_1_custom_fee,
        &acc_2.address(),
        vec![Coin::new(1, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap_err();

    assert_substring!(
        err.to_string(),
        SpendLimitError::overspend(1_500_000, 1_500_001).to_string()
    );
}
#[test]
fn test_fee_draining() {
    let app = OsmosisTestApp::new();
    set_maximum_unauthenticated_gas(&app, MAXIMUM_UNAUTHENTICATED_GAS);

    let initial_balance = [Coin::new(1_000_000_000_000_000, "uosmo")];
    let acc_1 = app.init_account(&initial_balance).unwrap();

    let acc_2 = app.init_account(&initial_balance).unwrap();

    let wasm = Wasm::new(&app);
    let bank = Bank::new(&app);

    // Store code and initialize spend limit contract
    let code_id = spend_limit_store_code(&wasm, &acc_2);
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
            admin: None,
        },
        &acc_2,
    );

    let spend_limit_querier = SpendLimitQuerier::new(&app, contract_addr.to_string());

    let acc_1_custom_fee = acc_1.with_fee_setting(FeeSetting::Custom {
        amount: Coin::new(5_000, "uosmo"),
        gas_limit: 1_000_000,
    });

    // Add spend limit authenticator
    let spend_limit_auth_id = add_spend_limit_authenticator(
        &app,
        &acc_1_custom_fee,
        &contract_addr,
        &SpendLimitParams {
            limit: Uint128::new(1_500_000),
            reset_period: Period::Day,
            time_limit: None,
        },
    );

    // create failed tx to spend fee
    let acc_1_custom_fee = acc_1_custom_fee.with_fee_setting(FeeSetting::Custom {
        amount: Coin::new(500_000, "uosmo"),
        gas_limit: 1_000_000,
    });
    bank_send(
        &app,
        &acc_1_custom_fee,
        &acc_1_custom_fee,
        &acc_2.address(),
        vec![Coin::new(1, "xxx")], // invalid denom
        spend_limit_auth_id,
    )
    .unwrap_err();

    // check that fee has been deducted
    let acc_1_balance = bank
        .query_balance(&QueryBalanceRequest {
            address: acc_1_custom_fee.address(),
            denom: "uosmo".to_string(),
        })
        .unwrap();
    assert_eq!(
        acc_1_balance.balance.unwrap(),
        Coin::new(1_000_000_000_000_000 - 500000 - 5000, "uosmo").into()
    );

    let acc_1_custom_fee = acc_1_custom_fee.with_fee_setting(FeeSetting::Custom {
        amount: Coin::new(1_000_000, "uosmo"),
        gas_limit: 1_000_000,
    });

    // spend to hit the limit, resulted in failed tx
    let err = bank_send(
        &app,
        &acc_1_custom_fee,
        &acc_1_custom_fee,
        &acc_2.address(),
        vec![Coin::new(1, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap_err();

    assert_substring!(
        err.to_string(),
        SpendLimitError::overspend(1500000, 1500001).to_string()
    );

    let latest_exec = app.get_block_time_nanos();

    // check that fee has been deducted
    let acc_1_balance = bank
        .query_balance(&QueryBalanceRequest {
            address: acc_1_custom_fee.address(),
            denom: "uosmo".to_string(),
        })
        .unwrap();
    assert_eq!(
        acc_1_balance.balance.unwrap(),
        Coin::new(1_000_000_000_000_000 - 1500000 - 5000, "uosmo").into()
    );

    // spending will not yet be updated (to be fixed)
    assert_eq!(
        spend_limit_querier
            .query_spendings_by_account(acc_1_custom_fee.address())
            .unwrap(),
        vec![(
            "1".to_string(),
            Spending {
                value_spent_in_period: 1500000u128.into(),
                last_spent_at: Timestamp::from_nanos(latest_exec as u64)
            }
        )]
    );

    // spend some more
    let acc_1_custom_fee = acc_1_custom_fee.with_fee_setting(FeeSetting::Custom {
        amount: Coin::new(2500, "uosmo"),
        gas_limit: 1_000_000,
    });
    let res = bank_send(
        &app,
        &acc_1_custom_fee,
        &acc_1_custom_fee,
        &acc_2.address(),
        vec![Coin::new(1, "uosmo")],
        spend_limit_auth_id,
    );
    assert_substring!(
        res.as_ref().unwrap_err().to_string(),
        SpendLimitError::overspend(1500000, 1502500).to_string()
    );

    // this should block at authenticate, which means fee shouldn't be deducted
    let acc_1_balance = bank
        .query_balance(&QueryBalanceRequest {
            address: acc_1_custom_fee.address(),
            denom: "uosmo".to_string(),
        })
        .unwrap();
    assert_eq!(
        acc_1_balance.balance.unwrap(),
        Coin::new(1_000_000_000_000_000 - 1500000 - 5000, "uosmo").into()
    );

    let prev_ts = app.get_block_time_seconds();
    let prev_dt = OffsetDateTime::from_unix_timestamp(prev_ts).unwrap();
    let next_dt = (prev_dt + Duration::days(1)).unix_timestamp();
    let diff = next_dt - prev_ts;

    app.increase_time(diff as u64);

    bank_send(
        &app,
        &acc_1_custom_fee,
        &acc_1_custom_fee,
        &acc_2.address(),
        vec![Coin::new(1_400_000, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    bank_send(
        &app,
        &acc_1_custom_fee,
        &acc_1_custom_fee,
        &acc_2.address(),
        vec![Coin::new(92_500, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    let err = bank_send(
        &app,
        &acc_1_custom_fee,
        &acc_1_custom_fee,
        &acc_2.address(),
        vec![Coin::new(1, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap_err();

    assert_substring!(
        err.to_string(),
        SpendLimitError::overspend(1_500_000, 1_500_001).to_string()
    );
}

#[test]
fn test_with_conversion() {
    let app = OsmosisTestApp::new();
    set_maximum_unauthenticated_gas(&app, MAXIMUM_UNAUTHENTICATED_GAS);

    let initial_balances = &[
        Coin::new(1_000_000_000_000_000, "uosmo"),
        Coin::new(1_000_000_000_000_000, "uion"),
        Coin::new(1_000_000_000_000_000, UUSDC),
        Coin::new(1_000_000_000_000_000, UATOM),
    ];
    let acc_1 = app.init_account(initial_balances).unwrap();

    let acc_2 = app.init_account(initial_balances).unwrap();

    let gamm = Gamm::new(&app);

    // 1:1.5
    let osmo_usdc_pool_id = gamm
        .create_basic_pool(
            &[
                Coin::new(1_000_000_000, "uosmo"),
                Coin::new(1_500_000_000, UUSDC),
            ],
            &acc_1,
        )
        .unwrap()
        .data
        .pool_id;

    // 4:1
    let ion_osmo_pool_id = gamm
        .create_basic_pool(
            &[Coin::new(4_000_000, "uion"), Coin::new(1_000_000, "uosmo")],
            &acc_1,
        )
        .unwrap()
        .data
        .pool_id;

    // 4:1
    let ion_atom_pool_id = gamm
        .create_basic_pool(
            &[Coin::new(4_000_000, "uion"), Coin::new(1_000_000, UATOM)],
            &acc_1,
        )
        .unwrap()
        .data
        .pool_id;

    // 1:1
    let atom_osmo_pool_id = gamm
        .create_basic_pool(
            &[Coin::new(1_000_000, UATOM), Coin::new(1_000_000, "uosmo")],
            &acc_1,
        )
        .unwrap()
        .data
        .pool_id;

    // increase time by 1h
    app.increase_time(3_600u64);

    let wasm = Wasm::new(&app);

    // Store code and initialize spend limit contract
    let code_id = spend_limit_store_code(&wasm, &acc_1);

    // try to instantiate with incorrect routes
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
                admin: None,
            },
            None,
            Some("spend_limit_authenticator"),
            &[],
            &acc_1,
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
            admin: None,
        },
        &acc_1,
    );

    // Add spend limit authenticator
    let spend_limit_auth_id = add_spend_limit_authenticator(
        &app,
        &acc_1,
        &contract_addr,
        &SpendLimitParams {
            limit: Uint128::new(1_000_000),
            reset_period: Period::Day,
            time_limit: None,
        },
    );

    let acc_1 = acc_1.with_fee_setting(FeeSetting::Custom {
        amount: Coin::new(2_500, "uosmo"),
        gas_limit: 1_000_000,
    });

    // spend to the limit
    bank_send(
        &app,
        &acc_1,
        &acc_1,
        &acc_2.address(),
        vec![Coin::new(666_666 - 2_500, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    // spend some more
    let res = bank_send(
        &app,
        &acc_1,
        &acc_1,
        &acc_2.address(),
        vec![Coin::new(1, "uosmo")],
        spend_limit_auth_id,
    );

    let fee_in_uusdc = 3750; // 3750uusdc = 2500usmo * 1,5
    assert_substring!(
        res.as_ref().unwrap_err().to_string(),
        SpendLimitError::overspend(1000000, 999_999 + fee_in_uusdc).to_string() // 999_999 is previous spend, this failed due to fee
    );

    let prev_ts = app.get_block_time_seconds();
    let prev_dt = OffsetDateTime::from_unix_timestamp(prev_ts).unwrap();
    let next_dt = (prev_dt + Duration::days(1)).unix_timestamp();
    let diff = next_dt - prev_ts;

    app.increase_time(diff as u64);

    bank_send(
        &app,
        &acc_1,
        &acc_1,
        &acc_2.address(),
        vec![Coin::new(500_000 - fee_in_uusdc, UUSDC)],
        spend_limit_auth_id,
    )
    .unwrap();

    bank_send(
        &app,
        &acc_1,
        &acc_1,
        &acc_2.address(),
        vec![Coin::new(499_999 - fee_in_uusdc, UUSDC)],
        spend_limit_auth_id,
    )
    .unwrap();

    let err = bank_send(
        &app,
        &acc_1,
        &acc_1,
        &acc_2.address(),
        vec![Coin::new(6, "uion")],
        spend_limit_auth_id,
    )
    .unwrap_err();

    assert_substring!(
        err.to_string(),
        SpendLimitError::overspend(1000000, 999_999 + fee_in_uusdc).to_string() // 999_999 is previous spend, this failed due to fee
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
            admin: None,
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

    let initial_balance = &[
        Coin::new(1_000_000_000_000_000, "uosmo"),
        Coin::new(1_000_000_000_000_000, "uion"),
        Coin::new(1_000_000_000_000_000, UUSDC),
        Coin::new(1_000_000_000_000_000, UATOM),
    ];
    let acc = app.init_account(initial_balance).unwrap();

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

    // try to instantiate with incorrect routes
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
            admin: None,
        },
        &acc,
    );

    let fee_in_osmo = 4000;
    let account_owner = acc.with_fee_setting(FeeSetting::Custom {
        amount: Coin::new(fee_in_osmo, "uosmo"),
        gas_limit: 1_000_000,
    });
    let one_click_trading_session_signer_1 = &empty_accs[0];
    let one_click_trading_session_pubkey_1 =
        one_click_trading_session_signer_1.public_key().to_bytes();

    let session_1_end = Timestamp::from_seconds(app.get_block_time_seconds() as u64).plus_days(3);

    let limit = 5_000_000_000;
    let one_click_trading_auth_id_1 = add_1ct_session_authenticator(
        &app,
        &account_owner,
        &one_click_trading_session_pubkey_1,
        &contract_addr,
        &SpendLimitParams {
            limit: limit.into(),
            reset_period: Period::Day,
            time_limit: Some(TimeLimit {
                start: None,
                end: session_1_end,
            }),
        },
    );

    // only `MsgSwapExactAmountIn` and `MsgSplitRouteSwapExactAmountIn` are allowed
    // failed at authenticate (which is in ante, prior to deduct fee) so no fee is deducted
    let err = bank_send(
        &app,
        &account_owner,
        one_click_trading_session_signer_1,
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
    // failed at authenticate (which is in ante, prior to deduct fee) so no fee is deducted
    let wrong_signer = &empty_accs[1];
    let err = one_click_swap_exact_amount_in(
        &app,
        &account_owner,
        wrong_signer,
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

    let osmo_out: u128 = app
        .query::<_, EstimateSwapExactAmountInResponse>(
            "/osmosis.poolmanager.v1beta1.Query/EstimateSwapExactAmountIn",
            #[allow(deprecated)] // pool id is deprecated
            &EstimateSwapExactAmountInRequest {
                pool_id: 0,
                token_in: format!("{}{}", 100, UUSDC),
                routes: vec![SwapAmountInRoute {
                    pool_id: osmo_usdc_pool_id,
                    token_out_denom: "uosmo".to_string(),
                }],
            },
        )
        .unwrap()
        .token_out_amount
        .parse()
        .unwrap();

    let uusdc_in = 100;

    one_click_swap_exact_amount_in(
        &app,
        &account_owner,
        one_click_trading_session_signer_1,
        vec![SwapAmountInRoute {
            pool_id: osmo_usdc_pool_id,
            token_out_denom: "uosmo".to_string(),
        }],
        Coin::new(uusdc_in, UUSDC),
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

    let s1_spent = uusdc_in + ((fee_in_osmo - osmo_out) * 15 / 10) + 1; // +1 due to rounding
    assert_eq!(spending.value_spent_in_period.u128(), s1_spent);

    let s1_remaining = limit - s1_spent;

    // swap ion to atom with overspend
    let fee_in_osmo = s1_remaining * 10 / 15;
    let account_owner = account_owner.with_fee_setting(FeeSetting::Custom {
        amount: Coin::new(fee_in_osmo, "uosmo"),
        gas_limit: 1_000_000,
    });

    let atom_out: u128 = app
        .query::<_, EstimateSwapExactAmountInResponse>(
            "/osmosis.poolmanager.v1beta1.Query/EstimateSwapExactAmountIn",
            #[allow(deprecated)] // pool id is deprecated
            &EstimateSwapExactAmountInRequest {
                pool_id: 0,
                token_in: format!("{}{}", 10, "uion"),
                routes: vec![SwapAmountInRoute {
                    pool_id: ion_atom_pool_id,
                    token_out_denom: UATOM.to_string(),
                }],
            },
        )
        .unwrap()
        .token_out_amount
        .parse()
        .unwrap();

    let ion_in_value = 4; // 10 * 1.5 / 4 = 3.75 ~> 4
    let atom_out_value = atom_out * 15 / 10; // atom_out * 1.5, atom out = 2 so no need to manually round
    let slippage = ion_in_value - atom_out_value;

    let err = one_click_swap_exact_amount_in(
        &app,
        &account_owner,
        one_click_trading_session_signer_1,
        vec![SwapAmountInRoute {
            pool_id: ion_atom_pool_id,
            token_out_denom: UATOM.to_string(),
        }],
        Coin::new(10, "uion"),
        1,
        one_click_trading_auth_id_1,
    )
    .unwrap_err();

    assert_substring!(
        err.to_string(),
        SpendLimitError::overspend(limit, s1_spent + s1_remaining + slippage).to_string()
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
        one_click_trading_session_signer_1,
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
        one_click_trading_session_signer_2,
        vec![SwapAmountInRoute {
            pool_id: ion_atom_pool_id,
            token_out_denom: UATOM.to_string(),
        }],
        Coin::new(10, "uion"),
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

    assert_eq!(
        spending.value_spent_in_period.u128(),
        s1_remaining + slippage
    );

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

    assert_eq!(spending.value_spent_in_period.u128(), limit);

    // increases time for 2 days
    app.increase_time(24 * 60 * 60 * 2);

    let SpendingResponse { spending } = wasm
        .query(
            &contract_addr,
            &QueryMsg::Spending {
                account: account_owner.address(),
                authenticator_id: format!("{one_click_trading_auth_id_1}.1"),
            },
        )
        .unwrap();

    assert_eq!(spending.value_spent_in_period.u128(), 0);

    // spend to the limit
    let uusdc_in: u128 = 10;
    let uosmo_out: u128 = app
        .query::<_, EstimateSwapExactAmountInResponse>(
            "/osmosis.poolmanager.v1beta1.Query/EstimateSwapExactAmountIn",
            #[allow(deprecated)] // pool id is deprecated
            &EstimateSwapExactAmountInRequest {
                pool_id: 0,
                token_in: format!("{}{}", uusdc_in, UUSDC),
                routes: vec![SwapAmountInRoute {
                    pool_id: osmo_usdc_pool_id,
                    token_out_denom: "uosmo".to_string(),
                }],
            },
        )
        .unwrap()
        .token_out_amount
        .parse()
        .unwrap();

    let slippage = uusdc_in - uosmo_out * 15 / 10;

    // uosmo twap price at this point is 1.5000002
    let account_owner = account_owner.with_fee_setting(FeeSetting::Custom {
        amount: Coin::new((limit - slippage) * 10000000 / 15000002, "uosmo"),
        gas_limit: 1_000_000,
    });

    one_click_swap_exact_amount_in(
        &app,
        &account_owner,
        one_click_trading_session_signer_1,
        vec![SwapAmountInRoute {
            pool_id: osmo_usdc_pool_id,
            token_out_denom: "uosmo".to_string(),
        }],
        Coin::new(uusdc_in, UUSDC),
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

    assert_eq!(spending.value_spent_in_period.u128(), limit);

    // increase time for 1 day, which the time limit for session 1 is over (3 days)
    app.increase_time(24 * 60 * 60);

    // try spend the last bit should fail due to time limit
    let err = one_click_swap_exact_amount_in(
        &app,
        &account_owner,
        one_click_trading_session_signer_1,
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
        "smartaccount",
        smartaccount::v1beta1::Params {
            maximum_unauthenticated_gas,
            is_smart_account_active: true,
            circuit_breaker_controllers: vec![],
        }
        .to_any(),
    )
    .unwrap();
}

struct SpendLimitQuerier<'a> {
    app: &'a OsmosisTestApp,
    contract_addr: String,
}

impl SpendLimitQuerier<'_> {
    fn new(app: &OsmosisTestApp, contract_addr: String) -> SpendLimitQuerier {
        SpendLimitQuerier { app, contract_addr }
    }
    fn query_spendings_by_account(&self, account: String) -> RunnerResult<Vec<(String, Spending)>> {
        let wasm = Wasm::new(self.app);
        let SpendingsByAccountResponse { spendings } = wasm.query(
            &self.contract_addr,
            &QueryMsg::SpendingsByAccount { account },
        )?;
        Ok(spendings)
    }
}

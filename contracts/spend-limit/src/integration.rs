// Ignore integration tests for code coverage since there will be problems with dynamic linking libosmosistesttube
// and also, tarpaulin will not be able read coverage out of wasm binary anyway
#![cfg(all(test, not(tarpaulin)))]

use cosmwasm_std::{Coin, Uint128};

use osmosis_std::types::{
    cosmos::bank::v1beta1::MsgSendResponse,
    osmosis::{
        authenticator::{MsgRemoveAuthenticator, MsgRemoveAuthenticatorResponse, TxExtension},
        poolmanager::v1beta1::SwapAmountInRoute,
    },
};
use osmosis_test_tube::{
    osmosis_std::types::cosmos::bank::v1beta1::MsgSend, Account, ExecuteResponse, Gamm, Module,
    OsmosisTestApp, Runner, RunnerError, SigningAccount, Wasm,
};
use time::{Duration, OffsetDateTime};

use crate::{
    assert_substring,
    msg::{InstantiateMsg, QueryMsg, SpendingsByAccountResponse, TrackedDenom},
    price::{PriceError, PriceResolutionConfig},
    spend_limit::{Period, SpendLimitError, SpendLimitParams, Spending},
    test_helper::authenticator_setup::{
        add_spend_limit_authenticator, spend_limit_instantiate, spend_limit_store_code,
    },
};

const UUSDC: &str = "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4";
const UATOM: &str = "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2";

#[test]
fn test_no_conversion() {
    let app = OsmosisTestApp::new();
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
        &accs[1].address(),
        vec![Coin::new(1_000_000, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    // spend some more
    let res = bank_send(
        &app,
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
        &accs[1].address(),
        vec![Coin::new(500_000, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    bank_send(
        &app,
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(499_999, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    let err = bank_send(
        &app,
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
        &accs[1].address(),
        vec![Coin::new(666_666, "uosmo")],
        spend_limit_auth_id,
    )
    .unwrap();

    // spend some more
    let res = bank_send(
        &app,
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
        &accs[1].address(),
        vec![Coin::new(500_000, UUSDC)],
        spend_limit_auth_id,
    )
    .unwrap();

    bank_send(
        &app,
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(499_999, UUSDC)],
        spend_limit_auth_id,
    )
    .unwrap();

    let err = bank_send(
        &app,
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
    add_spend_limit_authenticator(
        &app,
        &accs[0],
        &contract_addr,
        &SpendLimitParams {
            limit: Uint128::new(1_000_000),
            reset_period: Period::Day,
            time_limit: None,
        },
    );

    add_spend_limit_authenticator(
        &app,
        &accs[0],
        &contract_addr,
        &SpendLimitParams {
            limit: Uint128::new(999_999),
            reset_period: Period::Day,
            time_limit: None,
        },
    );

    add_spend_limit_authenticator(
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

fn bank_send(
    app: &OsmosisTestApp,
    from: &SigningAccount,
    to_address: &str,
    amount: Vec<Coin>,
    authenticator_id: u64,
) -> Result<ExecuteResponse<MsgSendResponse>, RunnerError> {
    let amount: Vec<osmosis_test_tube::osmosis_std::types::cosmos::base::v1beta1::Coin> =
        amount.into_iter().map(Into::into).collect();

    // a hack to set fee payer
    let self_send_to_set_fee_payer = (
        MsgSend {
            from_address: from.address(),
            to_address: from.address(),
            amount: amount.clone(),
        },
        MsgSend::TYPE_URL,
    );
    app.execute_multiple_custom_tx::<MsgSend, MsgSendResponse>(
        &[
            self_send_to_set_fee_payer,
            (
                MsgSend {
                    from_address: from.address(),
                    to_address: to_address.to_string(),
                    amount,
                },
                MsgSend::TYPE_URL,
            ),
        ],
        "",
        0u32,
        vec![],
        vec![TxExtension {
            // 0 is default authenticator, which is sigver for first signer
            // it will authenticate that as fee payer
            selected_authenticators: vec![0, authenticator_id as i32],
        }
        .to_any()
        .into()],
        from,
    )
}

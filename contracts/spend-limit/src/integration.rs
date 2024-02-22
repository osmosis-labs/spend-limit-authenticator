// Ignore integration tests for code coverage since there will be problems with dynamic linking libosmosistesttube
// and also, tarpaulin will not be able read coverage out of wasm binary anyway
#![cfg(all(test, not(tarpaulin)))]

use cosmwasm_std::Coin;

use osmosis_std::types::{
    cosmos::bank::v1beta1::MsgSendResponse,
    osmosis::{authenticator::TxExtension, poolmanager::v1beta1::SwapAmountInRoute},
};
use osmosis_test_tube::{
    osmosis_std::types::cosmos::bank::v1beta1::MsgSend, Account, ExecuteResponse, Gamm, Module,
    OsmosisTestApp, Runner, RunnerError, SigningAccount, Wasm,
};
use time::{Duration, OffsetDateTime};

use crate::{
    assert_substring,
    msg::{InstantiateMsg, TrackedDenom},
    price::PriceResolutionConfig,
    spend_limit::{Period, SpendLimitError, SpendLimitParams},
    test_helper::authenticator_setup::{
        add_sigver_authenticator, add_spend_limit_authenticator, spend_limit_instantiate,
        spend_limit_store_code,
    },
};

const UUSDC: &str = "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4";
const UATOM: &str = "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2";

#[test]
fn test_integration_no_conversion() {
    let app = OsmosisTestApp::new();
    let accs = app
        .init_accounts(&[Coin::new(1_000_000_000_000_000, "uosmo")], 2)
        .unwrap();

    // Add signature verification authenticator
    add_sigver_authenticator(&app, &accs[0]);

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
            authenticator_id: "2".to_string(),
            limit: Coin::new(1_000_000, "uosmo"),
            reset_period: Period::Day,
        },
    );

    // spend to the limit
    bank_send(
        &app,
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(1_000_000, "uosmo")],
    )
    .unwrap();

    // spend some more
    let res = bank_send(
        &app,
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(1, "uosmo")],
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
    )
    .unwrap();

    bank_send(
        &app,
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(499_999, "uosmo")],
    )
    .unwrap();

    let err = bank_send(
        &app,
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(2, "uosmo")],
    )
    .unwrap_err();

    assert_substring!(
        err.to_string(),
        SpendLimitError::overspend(1, 2).to_string()
    );
}

#[test]
fn test_integration_with_conversion() {
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

    // Add signature verification authenticator
    add_sigver_authenticator(&app, &accs[0]);

    let wasm = Wasm::new(&app);

    // Store code and initialize spend limit contract
    let code_id = spend_limit_store_code(&wasm, &accs[0]);
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
    add_spend_limit_authenticator(
        &app,
        &accs[0],
        &contract_addr,
        &SpendLimitParams {
            authenticator_id: "2".to_string(),
            limit: Coin::new(1_000_000, UUSDC),
            reset_period: Period::Day,
        },
    );

    // spend to the limit
    bank_send(
        &app,
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(666_666, "uosmo")],
    )
    .unwrap();

    // spend some more
    let res = bank_send(
        &app,
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(2, UUSDC)],
    );

    assert_substring!(
        res.as_ref().unwrap_err().to_string(),
        SpendLimitError::overspend(1, 2).to_string()
    );

    // TODO: test after reset
    let prev_ts = app.get_block_time_seconds() as i64;
    let prev_dt = OffsetDateTime::from_unix_timestamp(prev_ts).unwrap();
    let next_dt = (prev_dt + Duration::days(1)).unix_timestamp();
    let diff = next_dt - prev_ts;

    app.increase_time(diff as u64);

    // bank.send(
    //     MsgSend {
    //         from_address: accs[0].address(),
    //         to_address: accs[1].address(),
    //         amount: vec![Coin::new(500_000, "uosmo").into()],
    //     },
    //     &accs[0],
    // )
    // .unwrap();
    bank_send(
        &app,
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(500_000, UUSDC)],
    )
    .unwrap();

    bank_send(
        &app,
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(499_999, UUSDC)],
    )
    .unwrap();

    let err = bank_send(
        &app,
        &accs[0],
        &accs[1].address(),
        vec![Coin::new(6, "uion")],
    )
    .unwrap_err();

    assert_substring!(
        err.to_string(),
        SpendLimitError::overspend(1, 2).to_string()
    );
}

fn bank_send(
    app: &OsmosisTestApp,
    from: &SigningAccount,
    to_address: &str,
    amount: Vec<Coin>,
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
                    amount: amount,
                },
                MsgSend::TYPE_URL,
            ),
        ],
        "",
        0u32,
        vec![],
        vec![TxExtension {
            // assuption
            // - 1 = signature verification authenticator
            // - 2 = spend limit authenticator
            selected_authenticators: vec![0, 1],
        }
        .to_any()
        .into()],
        from,
    )
}

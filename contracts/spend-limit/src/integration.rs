#![cfg(test)]

use cosmwasm_std::Coin;

use osmosis_test_tube::{
    osmosis_std::types::cosmos::bank::v1beta1::MsgSend, Account, Bank, Module, OsmosisTestApp, Wasm,
};
use time::{Duration, OffsetDateTime};

use crate::{
    assert_substring,
    msg::InstantiateMsg,
    spend_limit::{Period, SpendLimitError, SpendLimitParams},
    test_helper::authenticator_setup::{
        add_signature_verification_authenticator, add_spend_limit_authenticator,
        spend_limit_instantiate, spend_limit_store_code,
    },
};

#[test]
fn test_integration() {
    let app = OsmosisTestApp::new();
    let accs = app
        .init_accounts(&[Coin::new(1_000_000_000_000_000, "uosmo")], 2)
        .unwrap();

    // Add signature verification authenticator
    add_signature_verification_authenticator(&app, &accs[0]);

    let wasm = Wasm::new(&app);
    // Store code and initialize mock price oracle contract
    let code_id = wasm
        .store_code(mock_cosmwasm_contract::WASM_BYTES, None, &accs[0])
        .unwrap()
        .data
        .code_id;

    let mock_price_oracle_contract_address = wasm
        .instantiate(
            code_id,
            &mock_cosmwasm_contract::InstantiateMsg {},
            None,
            Some("price_oracle"),
            &[],
            &accs[0],
        )
        .unwrap()
        .data
        .address;

    // Store code and initialize spend limit contract
    let code_id = spend_limit_store_code(&wasm, &accs[0]);
    let contract_addr = spend_limit_instantiate(
        &wasm,
        code_id,
        &InstantiateMsg {
            price_oracle_contract_addr: mock_price_oracle_contract_address,
        },
        &accs[0],
    );

    // Add spend limit authenticator
    add_spend_limit_authenticator(
        &app,
        &accs[0],
        &contract_addr,
        &SpendLimitParams {
            subkey: "subkey1".to_string(),
            limit: Coin::new(1_000_000, "uosmo"),
            reset_period: Period::Day,
        },
    );

    let bank = Bank::new(&app);

    // spend to the limit
    bank.send(
        MsgSend {
            from_address: accs[0].address(),
            to_address: accs[1].address(),
            amount: vec![Coin::new(1_000_000, "uosmo").into()],
        },
        &accs[0],
    )
    .unwrap();

    // spend some more
    let res = bank.send(
        MsgSend {
            from_address: accs[0].address(),
            to_address: accs[1].address(),
            amount: vec![Coin::new(1, "uosmo").into()],
        },
        &accs[0],
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

    bank.send(
        MsgSend {
            from_address: accs[0].address(),
            to_address: accs[1].address(),
            amount: vec![Coin::new(500_000, "uosmo").into()],
        },
        &accs[0],
    )
    .unwrap();

    bank.send(
        MsgSend {
            from_address: accs[0].address(),
            to_address: accs[1].address(),
            amount: vec![Coin::new(499_999, "uosmo").into()],
        },
        &accs[0],
    )
    .unwrap();

    let err = bank
        .send(
            MsgSend {
                from_address: accs[0].address(),
                to_address: accs[1].address(),
                amount: vec![Coin::new(2, "uosmo").into()],
            },
            &accs[0],
        )
        .unwrap_err();

    assert_substring!(
        err.to_string(),
        SpendLimitError::overspend(1, 2).to_string()
    );
}

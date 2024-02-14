#![cfg(test)]
use std::path::PathBuf;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Coin};
use osmosis_std::types::osmosis::authenticator::{
    MsgAddAuthenticator, MsgAddAuthenticatorResponse,
};
use osmosis_test_tube::{
    osmosis_std::types::cosmos::bank::v1beta1::MsgSend, Account, Bank, Module, OsmosisTestApp,
    Runner, Wasm,
};
use time::{Duration, OffsetDateTime};

use crate::{
    msg::InstantiateMsg,
    spend_limit::{Period, SpendLimitError, SpendLimitParams},
};

macro_rules! assert_substring {
    ($haystack:expr, $needle:expr) => {
        let Some(start) = $haystack.rfind($needle.as_str()) else {
            panic!(
                "Expected string:\n    {}\nnot found in:\n    `{}`",
                $needle, $haystack
            );
        };

        assert_eq!($haystack[start..start + $needle.len()], $needle);
    };
}

#[cw_serde]
struct CosmwasmAuthenticatorData {
    contract: String,
    params: Vec<u8>,
}

#[test]
fn test_integration() {
    let app = OsmosisTestApp::new();
    let accs = app
        .init_accounts(&[Coin::new(1_000_000_000_000_000, "uosmo")], 2)
        .unwrap();

    // Add signature verification authenticator
    let MsgAddAuthenticatorResponse { success } = app
        .execute(
            MsgAddAuthenticator {
                sender: accs[0].address(),
                r#type: "SignatureVerificationAuthenticator".to_string(),
                data: accs[0].public_key().to_bytes(),
            },
            MsgAddAuthenticator::TYPE_URL,
            &accs[0],
        )
        .unwrap()
        .data;

    assert!(success);

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
    let wasm_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("spend_limit.wasm");

    let wasm_bytes = std::fs::read(wasm_path).unwrap();

    let code_id = wasm
        .store_code(&wasm_bytes, None, &accs[0])
        .unwrap()
        .data
        .code_id;

    let contract_addr = wasm
        .instantiate(
            code_id,
            &InstantiateMsg {
                price_oracle_contract_addr: mock_price_oracle_contract_address,
            },
            None,
            Some("spend_limit_authenticator"),
            &[],
            &accs[0],
        )
        .unwrap()
        .data
        .address;

    // Add spend limit authenticator
    let data = CosmwasmAuthenticatorData {
        contract: contract_addr,
        params: to_json_binary(&SpendLimitParams {
            subkey: "default".to_string(),
            limit: Coin::new(1_000_000, "uosmo"),
            reset_period: Period::Day,
        })
        .unwrap()
        .to_vec(),
    };

    let MsgAddAuthenticatorResponse { success } = app
        .execute(
            MsgAddAuthenticator {
                sender: accs[0].address(),
                r#type: "CosmwasmAuthenticatorV1".to_string(),
                data: to_json_binary(&data).unwrap().to_vec(),
            },
            MsgAddAuthenticator::TYPE_URL,
            &accs[0],
        )
        .unwrap()
        .data;

    assert!(success);

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

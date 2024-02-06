#![cfg(test)]
use std::path::PathBuf;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Coin};
use osmosis_std::types::osmosis::authenticator::{
    MsgAddAuthenticator, MsgAddAuthenticatorResponse,
};
use osmosis_test_tube::{Account, Module, OsmosisTestApp, Runner, Wasm};

use crate::{
    msg::InstantiateMsg,
    spend_limit::{Period, SpendLimitParams},
};

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
            limit: Coin::new(1_000_000_000_000_000, "uosmo"),
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
}

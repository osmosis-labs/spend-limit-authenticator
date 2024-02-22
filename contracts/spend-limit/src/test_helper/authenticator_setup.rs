// Ignore integration tests for code coverage since there will be problems with dynamic linking libosmosistesttube
// and also, tarpaulin will not be able read coverage out of wasm binary anyway
#![cfg(all(test, not(tarpaulin)))]

use std::path::PathBuf;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::to_json_binary;
use osmosis_std::types::osmosis::authenticator::{
    MsgAddAuthenticator, MsgAddAuthenticatorResponse,
};
use osmosis_test_tube::{Account, OsmosisTestApp, Runner, SigningAccount, Wasm};

use crate::{msg::InstantiateMsg, spend_limit::SpendLimitParams};

#[cw_serde]
pub struct CosmwasmAuthenticatorData {
    pub contract: String,
    pub params: Vec<u8>,
}

pub fn add_sigver_authenticator(app: &OsmosisTestApp, acc: &SigningAccount) {
    let MsgAddAuthenticatorResponse { success } = app
        .execute(
            MsgAddAuthenticator {
                sender: acc.address(),
                r#type: "SignatureVerificationAuthenticator".to_string(),
                data: acc.public_key().to_bytes(),
            },
            MsgAddAuthenticator::TYPE_URL,
            acc,
        )
        .unwrap()
        .data;

    assert!(success);
}

pub fn spend_limit_store_code<'a>(wasm: &Wasm<'a, OsmosisTestApp>, acc: &SigningAccount) -> u64 {
    let wasm_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("spend_limit.wasm");

    let wasm_bytes = std::fs::read(wasm_path).unwrap();

    wasm.store_code(&wasm_bytes, None, acc)
        .unwrap()
        .data
        .code_id
}

pub fn spend_limit_instantiate<'a>(
    wasm: &Wasm<'a, OsmosisTestApp>,
    code_id: u64,
    msg: &InstantiateMsg,
    acc: &SigningAccount,
) -> String {
    wasm.instantiate(
        code_id,
        msg,
        None,
        Some("spend_limit_authenticator"),
        &[],
        acc,
    )
    .unwrap()
    .data
    .address
}

pub fn add_spend_limit_authenticator<'a>(
    app: &OsmosisTestApp,
    acc: &SigningAccount,
    contract: &str,
    params: &SpendLimitParams,
) {
    let data = CosmwasmAuthenticatorData {
        contract: contract.to_string(),
        params: to_json_binary(params).unwrap().to_vec(),
    };

    let MsgAddAuthenticatorResponse { success } = app
        .execute(
            MsgAddAuthenticator {
                sender: acc.address(),
                r#type: "CosmwasmAuthenticatorV1".to_string(),
                data: to_json_binary(&data).unwrap().to_vec(),
            },
            MsgAddAuthenticator::TYPE_URL,
            acc,
        )
        .unwrap()
        .data;

    assert!(success);
}

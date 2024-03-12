// Ignore integration tests for code coverage since there will be problems with dynamic linking libosmosistesttube
// and also, tarpaulin will not be able read coverage out of wasm binary anyway
#![cfg(all(test, not(tarpaulin)))]

use std::path::PathBuf;

use cosmwasm_std::to_json_binary;
use osmosis_std::types::osmosis::authenticator::{
    MsgAddAuthenticator, MsgAddAuthenticatorResponse,
};
use osmosis_test_tube::{Account, OsmosisTestApp, Runner, SigningAccount, Wasm};
use serde::{Deserialize, Serialize};

use crate::{msg::InstantiateMsg, spend_limit::SpendLimitParams};

#[derive(Serialize, Deserialize)]
pub struct CosmwasmAuthenticatorData {
    pub contract: String,
    pub params: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct SubAuthenticatorData {
    pub authenticator_type: String,
    pub data: Vec<u8>,
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
) -> u64 {
    add_authenticator(
        app,
        acc,
        "CosmwasmAuthenticatorV1",
        &CosmwasmAuthenticatorData {
            contract: contract.to_string(),
            params: to_json_binary(params).unwrap().to_vec(),
        },
    )
}

pub fn add_all_of_sig_ver_spend_limit_authenticator<'a>(
    app: &OsmosisTestApp,
    acc: &SigningAccount,
    contract: &str,
    params: &SpendLimitParams,
) -> u64 {
    add_authenticator(
        app,
        acc,
        "AllOfAuthenticator",
        &[
            SubAuthenticatorData {
                authenticator_type: "SignatureVerificationAuthenticator".to_string(),
                data: acc.public_key().to_bytes(),
            },
            SubAuthenticatorData {
                authenticator_type: "CosmwasmAuthenticatorV1".to_string(),
                data: to_json_binary(&CosmwasmAuthenticatorData {
                    contract: contract.to_string(),
                    params: to_json_binary(params).unwrap().to_vec(),
                })
                .unwrap()
                .to_vec(),
            },
        ],
    )
}

/// Add 1-click trading authenticator, it is
/// AllOf(
///     SignatureVerificationAuthenticator,
///     CosmwasmAuthenticatorV1(SpendLimit)),
///     AnyOf(
///         MessageFilterAuthenticator({"@type":"/osmosis.poolmanager.v1beta1.MsgSwapExactAmountIn"})
///         MessageFilterAuthenticator({"@type":"/osmosis.poolmanager.v1beta1.MsgSplitRouteSwapExactAmountIn"})
///     )
pub fn add_1ct_session_authenticator<'a>(
    app: &OsmosisTestApp,
    acc: &SigningAccount,
    session_pubkey: &[u8],
    spend_limit_contract: &str,
    params: &SpendLimitParams,
) -> u64 {
    add_authenticator(
        app,
        acc,
        "AllOfAuthenticator",
        &[
            SubAuthenticatorData {
                authenticator_type: "SignatureVerificationAuthenticator".to_string(),
                data: session_pubkey.to_vec(),
            },
            SubAuthenticatorData {
                authenticator_type: "CosmwasmAuthenticatorV1".to_string(),
                data: to_json_binary(&CosmwasmAuthenticatorData {
                    contract: spend_limit_contract.to_string(),
                    params: to_json_binary(params).unwrap().to_vec(),
                })
                .unwrap()
                .to_vec(),
            },
            SubAuthenticatorData {
                authenticator_type: "AnyOfAuthenticator".to_string(),
                data: to_json_binary(&[
                    SubAuthenticatorData {
                        authenticator_type: "MessageFilterAuthenticator".to_string(),
                        data: r#"{"@type":"/osmosis.poolmanager.v1beta1.MsgSwapExactAmountIn"}"#
                            .as_bytes()
                            .to_vec(),
                    },
                    SubAuthenticatorData {
                        authenticator_type: "MessageFilterAuthenticator".to_string(),
                        data: r#"{"@type":"/osmosis.poolmanager.v1beta1.MsgSplitRouteSwapExactAmountIn"}"#
                            .as_bytes()
                            .to_vec(),
                    },
                ])
                .unwrap()
                .to_vec(),
            },
        ],
    )
}

pub fn add_authenticator<T>(
    app: &OsmosisTestApp,
    acc: &SigningAccount,
    r#type: &str,
    data: &T,
) -> u64
where
    T: Serialize + ?Sized,
{
    let sender = acc.address();
    app.execute::<_, MsgAddAuthenticatorResponse>(
        MsgAddAuthenticator {
            sender,
            r#type: r#type.to_string(),
            data: to_json_binary(data).unwrap().to_vec(),
        },
        MsgAddAuthenticator::TYPE_URL,
        acc,
    )
    .unwrap()
    .events
    .into_iter()
    .find(|e| {
        e.ty == "message"
            && e.attributes
                .iter()
                .any(|a| a.key == "module" && a.value == "authenticator")
    })
    .unwrap()
    .attributes
    .into_iter()
    .find(|a| a.key == "authenticator_id")
    .unwrap()
    .value
    .parse::<u64>()
    .unwrap()
}

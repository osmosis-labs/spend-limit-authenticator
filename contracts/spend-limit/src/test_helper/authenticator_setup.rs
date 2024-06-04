// Ignore integration tests for code coverage since there will be problems with dynamic linking libosmosistesttube
// and also, tarpaulin will not be able read coverage out of wasm binary anyway
#![cfg(all(test, not(tarpaulin)))]

use std::path::PathBuf;

use cosmwasm_std::to_json_binary;
use osmosis_std::types::osmosis::smartaccount::v1beta1::{
    MsgAddAuthenticator, MsgAddAuthenticatorResponse,
};
use osmosis_test_tube::{Account, OsmosisTestApp, Runner, SigningAccount, Wasm};
use serde::Serialize;

use crate::{
    authenticator::{CosmwasmAuthenticatorData, SubAuthenticatorData},
    msg::InstantiateMsg,
    spend_limit::SpendLimitParams,
};

pub fn spend_limit_store_code(wasm: &Wasm<'_, OsmosisTestApp>, acc: &SigningAccount) -> u64 {
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

pub fn spend_limit_instantiate(
    wasm: &Wasm<'_, OsmosisTestApp>,
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

pub fn add_spend_limit_authenticator(
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

pub fn add_all_of_sig_ver_spend_limit_authenticator(
    app: &OsmosisTestApp,
    acc: &SigningAccount,
    contract: &str,
    params: &SpendLimitParams,
) -> u64 {
    add_authenticator(
        app,
        acc,
        "AllOf",
        &[
            SubAuthenticatorData {
                r#type: "SignatureVerification".to_string(),
                config: acc.public_key().to_bytes(),
            },
            SubAuthenticatorData {
                r#type: "CosmwasmAuthenticatorV1".to_string(),
                config: to_json_binary(&CosmwasmAuthenticatorData {
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
///     SignatureVerification,
///     CosmwasmAuthenticatorV1(SpendLimit)),
///     AnyOf(
///         MessageFilterAuthenticator({"@type":"/osmosis.poolmanager.v1beta1.MsgSwapExactAmountIn"})
///         MessageFilterAuthenticator({"@type":"/osmosis.poolmanager.v1beta1.MsgSplitRouteSwapExactAmountIn"})
///     )
pub fn add_1ct_session_authenticator(
    app: &OsmosisTestApp,
    acc: &SigningAccount,
    session_pubkey: &[u8],
    spend_limit_contract: &str,
    params: &SpendLimitParams,
) -> u64 {
    add_authenticator(
        app,
        acc,
        "AllOf",
        &[
            SubAuthenticatorData {
                r#type: "SignatureVerification".to_string(),
                config: session_pubkey.to_vec(),
            },
            SubAuthenticatorData {
                r#type: "CosmwasmAuthenticatorV1".to_string(),
                config: to_json_binary(&CosmwasmAuthenticatorData {
                    contract: spend_limit_contract.to_string(),
                    params: to_json_binary(params).unwrap().to_vec(),
                })
                .unwrap()
                .to_vec(),
            },
            SubAuthenticatorData {
                r#type: "AnyOf".to_string(),
                config: to_json_binary(&[
                    SubAuthenticatorData {
                        r#type: "MessageFilter".to_string(),
                        config: r#"{"@type":"/osmosis.poolmanager.v1beta1.MsgSwapExactAmountIn"}"#
                            .as_bytes()
                            .to_vec(),
                    },
                    SubAuthenticatorData {
                        r#type: "MessageFilter".to_string(),
                        config: r#"{"@type":"/osmosis.poolmanager.v1beta1.MsgSplitRouteSwapExactAmountIn"}"#
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
                .any(|a| a.key == "module" && a.value == "smartaccount")
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

use std::str::FromStr;

use cosmwasm_std::{from_json, StdError};
use osmosis_std::types::osmosis::smartaccount::v1beta1::AccountAuthenticator;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum CompositeAuthenticatorError {
    #[error("{0}")]
    StdError(#[from] StdError),

    #[error("Invalid composite id {composite_id}")]
    InvalidCompositeId { composite_id: String },

    #[error("Empty requested path")]
    EmptyRequestedPath,
}

impl CompositeAuthenticatorError {
    pub fn invalid_composite_id(composite_id: &str) -> Self {
        Self::InvalidCompositeId {
            composite_id: composite_id.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct CosmwasmAuthenticatorData {
    pub contract: String,

    #[serde(
        serialize_with = "crate::serde::as_base64_encoded_string::serialize",
        deserialize_with = "crate::serde::as_base64_encoded_string::deserialize"
    )]
    pub params: Vec<u8>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct SubAuthenticatorData {
    pub r#type: String,

    #[serde(
        serialize_with = "crate::serde::as_base64_encoded_string::serialize",
        deserialize_with = "crate::serde::as_base64_encoded_string::deserialize"
    )]
    pub config: Vec<u8>,
}

#[derive(PartialEq, Eq, Debug)]
pub struct CompositeId {
    /// Root authenticator id
    pub root: u64,

    /// Path to target authenticator
    pub path: Vec<usize>,
}

impl CompositeId {
    pub fn new(root: u64, path: Vec<usize>) -> Self {
        CompositeId { root, path }
    }
}

impl FromStr for CompositeId {
    type Err = CompositeAuthenticatorError;

    fn from_str(composite_id: &str) -> Result<Self, Self::Err> {
        let mut parts = composite_id.split('.');
        let root = parts
            .next()
            .map(|s| s.parse::<u64>())
            .ok_or_else(|| CompositeAuthenticatorError::invalid_composite_id(composite_id))?
            .map_err(|_| CompositeAuthenticatorError::invalid_composite_id(composite_id))?;

        let path = parts
            .map(|s| s.parse::<usize>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| CompositeAuthenticatorError::invalid_composite_id(composite_id))?;

        Ok(CompositeId { root, path })
    }
}

impl ToString for CompositeId {
    fn to_string(&self) -> String {
        let path = self
            .path
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(".");

        if path.is_empty() {
            self.root.to_string()
        } else {
            format!("{}.{}", self.root, path)
        }
    }
}

pub trait CompositeAuthenticator {
    fn child_authenticator_data<T>(self, path: &[usize]) -> Result<T, CompositeAuthenticatorError>
    where
        T: DeserializeOwned;
}

impl CompositeAuthenticator for AccountAuthenticator {
    fn child_authenticator_data<T>(self, path: &[usize]) -> Result<T, CompositeAuthenticatorError>
    where
        T: DeserializeOwned,
    {
        if path.is_empty() {
            return from_json(self.config).map_err(CompositeAuthenticatorError::StdError);
        }

        let root_sub_auths: Vec<SubAuthenticatorData> = from_json(self.config)?;
        let sub_auths =
            path.iter()
                .take(path.len() - 1)
                .try_fold(root_sub_auths, |parent, &p| {
                    from_json(
                        parent
                            .get(p)
                            .ok_or_else(|| {
                                CompositeAuthenticatorError::invalid_composite_id(
                                    CompositeId::new(self.id, path.to_vec())
                                        .to_string()
                                        .as_str(),
                                )
                            })?
                            .config
                            .as_slice(),
                    )
                    .map_err(CompositeAuthenticatorError::StdError)
                })?;

        let target_idx = path
            .last()
            .ok_or(CompositeAuthenticatorError::EmptyRequestedPath)?;

        let target_data = sub_auths
            .get(*target_idx)
            .ok_or_else(|| {
                CompositeAuthenticatorError::invalid_composite_id(
                    CompositeId::new(self.id, path.to_vec())
                        .to_string()
                        .as_str(),
                )
            })?
            .config
            .as_slice();

        from_json(target_data).map_err(CompositeAuthenticatorError::StdError)
    }
}

#[cfg(test)]
mod tests {
    use crate::{period::Period, spend_limit::SpendLimitParams};

    use super::*;
    use cosmwasm_std::to_json_vec;
    use rstest::rstest;

    #[rstest]
    #[case("1", Ok(CompositeId { root: 1, path: vec![] }))]
    #[case("1.2", Ok(CompositeId { root: 1, path: vec![2] }))]
    #[case("1.2.3", Ok(CompositeId { root: 1, path: vec![2, 3] }))]
    #[case("904.2.3", Ok(CompositeId { root: 904, path: vec![2, 3] }))]
    #[case("54.666.2.1", Ok(CompositeId { root: 54, path: vec![666, 2, 1] }))]
    #[case(
        "1,2,3",
        Err(CompositeAuthenticatorError::invalid_composite_id(composite_id))
    )]
    #[case(
        "1.x.3",
        Err(CompositeAuthenticatorError::invalid_composite_id(composite_id))
    )]
    #[case(
        "abc",
        Err(CompositeAuthenticatorError::invalid_composite_id(composite_id))
    )]
    fn test_composite_id_from_str(
        #[case] composite_id: &str,
        #[case] expected: Result<CompositeId, CompositeAuthenticatorError>,
    ) {
        let result = CompositeId::from_str(composite_id);
        assert_eq!(result, expected);

        if result.is_ok() {
            assert_eq!(result.unwrap().to_string(), composite_id);
        }
    }

    #[test]
    fn test_child_authenticator_data() {
        let params = SpendLimitParams {
            limit: 1000000u128.into(),
            reset_period: Period::Day,
            time_limit: None,
        };
        // no depth
        let target_data = CosmwasmAuthenticatorData {
            contract: "contract".to_string(),
            params: to_json_vec(&params).unwrap(),
        };
        let account_auth = AccountAuthenticator {
            config: to_json_vec(&target_data).unwrap(),
            id: 1,
            r#type: "AllOf".to_string(),
        };

        let result: Result<CosmwasmAuthenticatorData, CompositeAuthenticatorError> =
            account_auth.child_authenticator_data(&[]);
        assert_eq!(result.unwrap(), target_data);

        // depth 1

        let target_data = CosmwasmAuthenticatorData {
            contract: "contract".to_string(),
            params: to_json_vec(&params).unwrap(),
        };
        let account_auth = AccountAuthenticator {
            config: to_json_vec(&vec![
                SubAuthenticatorData {
                    r#type: "Dummy".to_string(),
                    config: vec![],
                },
                SubAuthenticatorData {
                    r#type: "CosmWasmAuthenticatorV1".to_string(),
                    config: to_json_vec(&target_data).unwrap(),
                },
            ])
            .unwrap(),
            id: 1,
            r#type: "AllOf".to_string(),
        };

        let result: Result<CosmwasmAuthenticatorData, CompositeAuthenticatorError> =
            account_auth.child_authenticator_data(&[1]);
        assert_eq!(result.unwrap(), target_data);

        // more depth

        let target_data = CosmwasmAuthenticatorData {
            contract: "contract".to_string(),
            params: to_json_vec(&params).unwrap(),
        };

        let account_auth = AccountAuthenticator {
            config: to_json_vec(&vec![
                SubAuthenticatorData {
                    r#type: "AnyOf".to_string(),
                    config: to_json_vec(&vec![
                        SubAuthenticatorData {
                            r#type: "Dummy".to_string(),
                            config: vec![],
                        },
                        SubAuthenticatorData {
                            r#type: "CosmWasmAuthenticatorV1".to_string(),
                            config: to_json_vec(&target_data).unwrap(),
                        },
                    ])
                    .unwrap(),
                },
                SubAuthenticatorData {
                    r#type: "Dummy".to_string(),
                    config: vec![],
                },
                SubAuthenticatorData {
                    r#type: "Dummy".to_string(),
                    config: vec![],
                },
            ])
            .unwrap(),
            id: 1,
            r#type: "AllOf".to_string(),
        };

        let result: Result<CosmwasmAuthenticatorData, CompositeAuthenticatorError> =
            account_auth.clone().child_authenticator_data(&[0, 1]);
        assert_eq!(result.unwrap(), target_data);

        let result: Result<CosmwasmAuthenticatorData, CompositeAuthenticatorError> =
            account_auth.clone().child_authenticator_data(&[0, 2]);
        assert_eq!(
            result.unwrap_err(),
            CompositeAuthenticatorError::invalid_composite_id("1.0.2")
        );

        let result: Result<CosmwasmAuthenticatorData, CompositeAuthenticatorError> =
            account_auth.child_authenticator_data(&[10]);
        assert_eq!(
            result.unwrap_err(),
            CompositeAuthenticatorError::invalid_composite_id("1.10")
        );
    }

    #[test]
    fn test_child_authenticator_data_with_raw_data() {
        let account_auth = AccountAuthenticator {
            config: r#"[{"type":"SignatureVerification","config":"ApNMBAr8lFRS6DaOKXgGXFcrpf78KHyqPvRCLZrM0Zzg"},{"type":"CosmwasmAuthenticatorV1","config":"eyJjb250cmFjdCI6ICJvc21vMTZlbDg3dGZ6Y3F3YWVxcmE3ZTV5M2h4ZHhnMmo1Zzh5cGZkMHBtdWt1eGc4MjJsaDVyY3FodWd1N2QiLCAicGFyYW1zIjogImV5SjBhVzFsWDJ4cGJXbDBJam9nZXlKbGJtUWlPaUFpTVRjeE56UTFNVGcxTmpVd05EQTJOVFl3TWlKOUxDQWljbVZ6WlhSZmNHVnlhVzlrSWpvZ0ltUmhlU0lzSUNKc2FXMXBkQ0k2SUNJeE1EQXdNQ0o5In0="},{"type":"MessageFilter","config":"eyJAdHlwZSI6Ii9vc21vc2lzLnBvb2xtYW5hZ2VyLnYxYmV0YTEuTXNnU3dhcEV4YWN0QW1vdW50SW4ifQ=="}]"#.as_bytes().to_vec(),
            id: 5,
            r#type: "AllOf".to_string(),
        };

        let cosmwasm_auth_data: CosmwasmAuthenticatorData =
            account_auth.clone().child_authenticator_data(&[1]).unwrap();

        assert_eq!(cosmwasm_auth_data, CosmwasmAuthenticatorData {
            contract: "osmo16el87tfzcqwaeqra7e5y3hxdxg2j5g8ypfd0pmukuxg822lh5rcqhugu7d".to_string(),
            params: r#"{"time_limit": {"end": "1717451856504065602"}, "reset_period": "day", "limit": "10000"}"#.as_bytes().to_vec(),
        });
    }
}

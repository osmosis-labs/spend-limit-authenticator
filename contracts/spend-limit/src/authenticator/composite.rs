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
    pub params: Vec<u8>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct SubAuthenticatorData {
    pub r#type: String,
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

    /// This is returned from the GetAuthenticator GRPCQuery.
    /// *github.com/osmosis-labs/osmosis/v25/x/smart-account/types.AccountAuthenticator {
    /// 	Id: 5,
    /// 	Type: "AllOf",
    /// 	Config: []uint8 len: 538, cap: 576, [91,123,34,116,121,112,101,34,58,34,83,105,103,110,97,116,117,114,101,86,101,114,105,102,105,99,97,116,105,111,110,34,44,34,99,111,110,102,105,103,34,58,34,65,112,78,77,66,65,114,56,108,70,82,83,54,68,97,79,75,88,103,71,88,70,99,114,112,102,55,56,75,72,121,113,80,118,82,67,76,90,114,77,48,90,122,103,34,125,44,123,34,116,121,112,101,34,58,34,67,111,115,109,119,97,115,109,65,117,116,104,101,110,116,105,99,97,116,111,114,86,49,34,44,34,99,111,110,102,105,103,34,58,34,101,121,74,106,98,50,53,48,99,109,70,106,100,67,73,54,73,67,74,118,99,50,49,118,77,84,90,108,98,68,103,51,100,71,90,54,89,51,70,51,89,87,86,120,99,109,69,51,90,84,86,53,77,50,104,52,90,72,104,110,77,109,111,49,90,122,104,53,99,71,90,107,77,72,66,116,100,87,116,49,101,71,99,52,77,106,74,115,97,68,86,121,89,51,70,111,100,87,100,49,78,50,81,105,76,67,65,105,99,71,70,121,89,87,49,122,73,106,111,103,73,109,86,53,83,106,66,104,86,122,70,115,87,68,74,52,99,71,74,88,98,68,66,74,97,109,57,110,90,88,108,75,98,71,74,116,85,87,108,80,97,85,70,112,84,86,82,106,101,69,53,54,85,84,70,78,86,71,99,120,84,109,112,86,100,48,53,69,81,84,74,79,86,70,108,51,84,87,108,75,79,85,120,68,81,87,108,106,98,86,90,54,87,108,104,83,90,109,78,72,86,110,108,104,86,122,108,114,83,87,112,118,90,48,108,116,85,109,104,108,85,48,108,122,83,85,78,75,99,50,70,88,77,88,66,107,81,48,107,50,83,85,78,74,101,69,49,69,81,88,100,78,81,48,111,53,73,110,48,61,34,125,44,123,34,116,121,112,101,34,58,34,77,101,115,115,97,103,101,70,105,108,116,101,114,34,44,34,99,111,110,102,105,103,34,58,34,101,121,74,65,100,72,108,119,90,83,73,54,73,105,57,118,99,50,49,118,99,50,108,122,76,110,66,118,98,50,120,116,89,87,53,104,90,50,86,121,76,110,89,120,89,109,86,48,89,84,69,117,84,88,78,110,85,51,100,104,99,69,86,52,89,87,78,48,81,87,49,118,100,87,53,48,83,87,52,105,102,81,61,61,34,125,93],}
    /// 	'[{"type":"SignatureVerification","config":"ApNMBAr8lFRS6DaOKXgGXFcrpf78KHyqPvRCLZrM0Zzg"},{"type":"CosmwasmAuthenticatorV1","config":"eyJjb250cmFjdCI6ICJvc21vMTZlbDg3dGZ6Y3F3YWVxcmE3ZTV5M2h4ZHhnMmo1Zzh5cGZkMHBtdWt1eGc4MjJsaDVyY3FodWd1N2QiLCAicGFyYW1zIjogImV5SjBhVzFsWDJ4cGJXbDBJam9nZXlKbGJtUWlPaUFpTVRjeE56UTFNVGcxTmpVd05EQTJOVFl3TWlKOUxDQWljbVZ6WlhSZmNHVnlhVzlrSWpvZ0ltUmhlU0lzSUNKc2FXMXBkQ0k2SUNJeE1EQXdNQ0o5In0="},{"type":"MessageFilter","config":"eyJAdHlwZSI6Ii9vc21vc2lzLnBvb2xtYW5hZ2VyLnYxYmV0YTEuTXNnU3dhcEV4YWN0QW1vdW50SW4ifQ=="}]'
    #[test]
    fn test_child_authenticator_data_from_golang() {
        let account_auth = AccountAuthenticator {
            config: vec![
                91, 123, 34, 116, 121, 112, 101, 34, 58, 34, 83, 105, 103, 110, 97, 116, 117, 114,
                101, 86, 101, 114, 105, 102, 105, 99, 97, 116, 105, 111, 110, 34, 44, 34, 99, 111,
                110, 102, 105, 103, 34, 58, 34, 65, 112, 78, 77, 66, 65, 114, 56, 108, 70, 82, 83,
                54, 68, 97, 79, 75, 88, 103, 71, 88, 70, 99, 114, 112, 102, 55, 56, 75, 72, 121,
                113, 80, 118, 82, 67, 76, 90, 114, 77, 48, 90, 122, 103, 34, 125, 44, 123, 34, 116,
                121, 112, 101, 34, 58, 34, 67, 111, 115, 109, 119, 97, 115, 109, 65, 117, 116, 104,
                101, 110, 116, 105, 99, 97, 116, 111, 114, 86, 49, 34, 44, 34, 99, 111, 110, 102,
                105, 103, 34, 58, 34, 101, 121, 74, 106, 98, 50, 53, 48, 99, 109, 70, 106, 100, 67,
                73, 54, 73, 67, 74, 118, 99, 50, 49, 118, 77, 84, 90, 108, 98, 68, 103, 51, 100,
                71, 90, 54, 89, 51, 70, 51, 89, 87, 86, 120, 99, 109, 69, 51, 90, 84, 86, 53, 77,
                50, 104, 52, 90, 72, 104, 110, 77, 109, 111, 49, 90, 122, 104, 53, 99, 71, 90, 107,
                77, 72, 66, 116, 100, 87, 116, 49, 101, 71, 99, 52, 77, 106, 74, 115, 97, 68, 86,
                121, 89, 51, 70, 111, 100, 87, 100, 49, 78, 50, 81, 105, 76, 67, 65, 105, 99, 71,
                70, 121, 89, 87, 49, 122, 73, 106, 111, 103, 73, 109, 86, 53, 83, 106, 66, 104, 86,
                122, 70, 115, 87, 68, 74, 52, 99, 71, 74, 88, 98, 68, 66, 74, 97, 109, 57, 110, 90,
                88, 108, 75, 98, 71, 74, 116, 85, 87, 108, 80, 97, 85, 70, 112, 84, 86, 82, 106,
                101, 69, 53, 54, 85, 84, 70, 78, 86, 71, 99, 120, 84, 109, 112, 86, 100, 48, 53,
                69, 81, 84, 74, 79, 86, 70, 108, 51, 84, 87, 108, 75, 79, 85, 120, 68, 81, 87, 108,
                106, 98, 86, 90, 54, 87, 108, 104, 83, 90, 109, 78, 72, 86, 110, 108, 104, 86, 122,
                108, 114, 83, 87, 112, 118, 90, 48, 108, 116, 85, 109, 104, 108, 85, 48, 108, 122,
                83, 85, 78, 75, 99, 50, 70, 88, 77, 88, 66, 107, 81, 48, 107, 50, 83, 85, 78, 74,
                101, 69, 49, 69, 81, 88, 100, 78, 81, 48, 111, 53, 73, 110, 48, 61, 34, 125, 44,
                123, 34, 116, 121, 112, 101, 34, 58, 34, 77, 101, 115, 115, 97, 103, 101, 70, 105,
                108, 116, 101, 114, 34, 44, 34, 99, 111, 110, 102, 105, 103, 34, 58, 34, 101, 121,
                74, 65, 100, 72, 108, 119, 90, 83, 73, 54, 73, 105, 57, 118, 99, 50, 49, 118, 99,
                50, 108, 122, 76, 110, 66, 118, 98, 50, 120, 116, 89, 87, 53, 104, 90, 50, 86, 121,
                76, 110, 89, 120, 89, 109, 86, 48, 89, 84, 69, 117, 84, 88, 78, 110, 85, 51, 100,
                104, 99, 69, 86, 52, 89, 87, 78, 48, 81, 87, 49, 118, 100, 87, 53, 48, 83, 87, 52,
                105, 102, 81, 61, 61, 34, 125, 93,
            ],
            id: 1,
            r#type: "AllOf".to_string(),
        };

        let print_result: Result<CosmwasmAuthenticatorData, CompositeAuthenticatorError> =
            account_auth.clone().child_authenticator_data(&[]);
        dbg!("{:?}", print_result);
    }
}

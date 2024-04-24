use std::str::FromStr;

use cosmwasm_std::{from_json, StdError};
use osmosis_std::types::osmosis::smartaccount::v1beta1::AccountAuthenticator;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::spend_limit::SpendLimitParams;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum CompositeAuthenticatorError {
    #[error("{0}")]
    StdError(#[from] StdError),

    #[error("Invalid composite id {composite_id}")]
    InvalidCompositeId { composite_id: String },
}

impl CompositeAuthenticatorError {
    pub fn invalid_composite_id(composite_id: &str) -> Self {
        Self::InvalidCompositeId {
            composite_id: composite_id.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct SpendLimitCosmwasmAuthenticatorData {
    pub contract: String,
    pub params: SpendLimitParams,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct SubAuthenticatorData {
    pub authenticator_type: String,
    pub data: Vec<u8>,
}

#[derive(PartialEq, Eq, Debug)]
pub struct CompositeId {
    /// Root authenticator id
    pub root: u64,

    /// Path to target authenticator
    pub path: Vec<u64>,
}

impl FromStr for CompositeId {
    type Err = CompositeAuthenticatorError;

    fn from_str(composite_id: &str) -> Result<Self, Self::Err> {
        let mut parts = composite_id.split('.').map(|s| s.parse::<u64>());
        let root = parts
            .next()
            .ok_or_else(|| CompositeAuthenticatorError::invalid_composite_id(composite_id))?
            .map_err(|_| CompositeAuthenticatorError::invalid_composite_id(composite_id))?;

        let path = parts
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| CompositeAuthenticatorError::invalid_composite_id(composite_id))?;

        Ok(CompositeId { root, path })
    }
}

pub trait CompositeAuthenticator {
    fn child_authenticator_data<T>(self, path: &[u64]) -> Result<T, CompositeAuthenticatorError>
    where
        T: DeserializeOwned;
}

impl CompositeAuthenticator for AccountAuthenticator {
    fn child_authenticator_data<T>(self, path: &[u64]) -> Result<T, CompositeAuthenticatorError>
    where
        T: DeserializeOwned,
    {
        let root_sub_auths: Vec<SubAuthenticatorData> = from_json(self.data)?;
        let sub_auths = path
            .iter()
            .take(path.len() - 1)
            .try_fold(root_sub_auths, |parent, &p| {
                from_json(parent[usize::try_from(p).unwrap()].data.as_slice())
            })?;

        from_json(sub_auths[*path.last().unwrap() as usize].data.as_slice())
            .map_err(CompositeAuthenticatorError::StdError)
    }
}

#[cfg(test)]
mod tests {
    use crate::period::Period;

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
    }

    #[test]
    fn test_child_authenticator_data() {
        let target_data = SpendLimitCosmwasmAuthenticatorData {
            contract: "contract".to_string(),
            params: SpendLimitParams {
                limit: 1000000u128.into(),
                reset_period: Period::Day,
                time_limit: None,
            },
        };
        let account_auth = AccountAuthenticator {
            data: to_json_vec(&vec![
                SubAuthenticatorData {
                    authenticator_type: "Dummy".to_string(),
                    data: vec![],
                },
                SubAuthenticatorData {
                    authenticator_type: "CosmWasmAuthenticatorV1".to_string(),
                    data: to_json_vec(&target_data).unwrap(),
                },
            ])
            .unwrap(),
            id: 1,
            r#type: "AllOf".to_string(),
        };

        let result: Result<SpendLimitCosmwasmAuthenticatorData, CompositeAuthenticatorError> =
            account_auth.child_authenticator_data(&[1]);
        assert_eq!(result.unwrap(), target_data);

        // more depth

        let target_data = SpendLimitCosmwasmAuthenticatorData {
            contract: "contract".to_string(),
            params: SpendLimitParams {
                limit: 1000000u128.into(),
                reset_period: Period::Day,
                time_limit: None,
            },
        };

        let account_auth = AccountAuthenticator {
            data: to_json_vec(&vec![
                SubAuthenticatorData {
                    authenticator_type: "AnyOf".to_string(),
                    data: to_json_vec(&vec![
                        SubAuthenticatorData {
                            authenticator_type: "Dummy".to_string(),
                            data: vec![],
                        },
                        SubAuthenticatorData {
                            authenticator_type: "CosmWasmAuthenticatorV1".to_string(),
                            data: to_json_vec(&target_data).unwrap(),
                        },
                    ])
                    .unwrap(),
                },
                SubAuthenticatorData {
                    authenticator_type: "Dummy".to_string(),
                    data: vec![],
                },
                SubAuthenticatorData {
                    authenticator_type: "Dummy".to_string(),
                    data: vec![],
                },
            ])
            .unwrap(),
            id: 1,
            r#type: "AllOf".to_string(),
        };

        let result: Result<SpendLimitCosmwasmAuthenticatorData, CompositeAuthenticatorError> =
            account_auth.child_authenticator_data(&[0, 1]);
        assert_eq!(result.unwrap(), target_data);
    }
}

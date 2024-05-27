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
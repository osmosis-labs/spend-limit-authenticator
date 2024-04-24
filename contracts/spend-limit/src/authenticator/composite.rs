use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::AuthenticatorError;

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

#[derive(PartialEq, Eq, Debug)]
pub struct CompositeId {
    /// Root authenticator id
    pub root: u64,

    /// Path to target authenticator
    pub path: Vec<u64>,
}

impl FromStr for CompositeId {
    type Err = AuthenticatorError;

    fn from_str(composite_id: &str) -> Result<Self, Self::Err> {
        let mut parts = composite_id.split('.').map(|s| s.parse::<u64>());
        let root = parts
            .next()
            .ok_or_else(|| AuthenticatorError::invalid_composite_id(composite_id))?
            .map_err(|_| AuthenticatorError::invalid_composite_id(composite_id))?;

        let path = parts
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AuthenticatorError::invalid_composite_id(composite_id))?;

        Ok(CompositeId { root, path })
    }
}

// TODO:
// - from composite id, get base id, query authenticator with that.
// - from there, if it's a composite, parse data as []SubAuthenticatorData should pass
// - if it's a cosmwasm, parse data as CosmwasmAuthenticatorData and pass (which is expected for spend limit case)

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("1", Ok(CompositeId { root: 1, path: vec![] }))]
    #[case("1.2", Ok(CompositeId { root: 1, path: vec![2] }))]
    #[case("1.2.3", Ok(CompositeId { root: 1, path: vec![2, 3] }))]
    #[case("904.2.3", Ok(CompositeId { root: 904, path: vec![2, 3] }))]
    #[case("54.666.2.1", Ok(CompositeId { root: 54, path: vec![666, 2, 1] }))]
    #[case("1,2,3", Err(AuthenticatorError::invalid_composite_id(composite_id)))]
    #[case("1.x.3", Err(AuthenticatorError::invalid_composite_id(composite_id)))]
    #[case("abc", Err(AuthenticatorError::invalid_composite_id(composite_id)))]
    fn test_composite_id_from_str(
        #[case] composite_id: &str,
        #[case] expected: Result<CompositeId, AuthenticatorError>,
    ) {
        let result = CompositeId::from_str(composite_id);
        assert_eq!(result, expected);
    }
}

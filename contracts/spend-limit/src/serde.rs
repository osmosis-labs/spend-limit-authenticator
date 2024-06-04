pub mod as_base64_encoded_string {
    use cosmwasm_std::Binary;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded_string = String::deserialize(deserializer)?;
        Binary::from_base64(&encoded_string)
            .map(|b| b.to_vec())
            .map_err(de::Error::custom)
    }

    pub fn serialize<S>(values: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Binary(values.to_vec()).to_base64().serialize(serializer)
    }
}

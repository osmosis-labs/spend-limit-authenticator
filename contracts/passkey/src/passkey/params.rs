use cosmwasm_schema::cw_serde;

#[cw_serde]
pub struct PasskeyParams {
    pub seal: Vec<u8>,
    pub jwt: Vec<u8>,
    pub attestation: Vec<u8>,
    pub app_check_jwt: Vec<u8>,
    pub passkey_name: Vec<u8>,
    pub raw_client_data_jsonb64: Vec<u8>,
    pub raw_authenticator_data_b64: Vec<u8>,
    pub signature_b64: Vec<u8>,
}

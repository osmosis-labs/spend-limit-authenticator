use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum PasskeyError {
    #[error("{0}")]
    Std(#[from] cosmwasm_std::StdError),
}

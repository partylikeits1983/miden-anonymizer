use miden_protocol::errors::AccountError;

#[derive(Debug, thiserror::Error)]
pub enum PtaError {
    #[error("failed to build pass-through account")]
    AccountBuild(#[source] AccountError),
}

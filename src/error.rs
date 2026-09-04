use thiserror::Error;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum BacktestError {
    #[error("account ran out of money")]
    OutOfMoney,
    #[error("invalid order: {0}")]
    InvalidOrder(String),
    #[error("invalid paramter: {0}")]
    InvalidParameter(String),
    #[error("invalid strategy paramter: {0}")]
    UnknownStrategyParameter(String),
    #[error("indicator error: {0}")]
    IndicatorError(String),
    #[error("{0}")]
    Other(String),
}

pub type BtResult<T> = Result<T, BacktestError>;

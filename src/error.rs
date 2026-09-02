use std::fmt;

#[derive(Debug)]
pub enum BacktestError {
    OutOfMoney,
    InvalidOrder(String),
    InvalidParameter(String),
    UnknownStrategyParameter(String),
    IndicatorError(String),
    Other(String),
}
impl fmt::Display for BacktestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BacktestError::OutOfMoney => write!(f, "account run out of money"),
            BacktestError::InvalidOrder(s) => write!(f, "invalid order {s}"),
            BacktestError::InvalidParameter(s) => write!(f, "invalid parameter {s}"),
            BacktestError::UnknownStrategyParameter(s) => {
                write!(f, "unknown strategy parameter {s}")
            }
            BacktestError::IndicatorError(i) => write!(f, "indicator error {i}"),
            BacktestError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for BacktestError {}
pub type BtResult<T> = Result<T, BacktestError>;

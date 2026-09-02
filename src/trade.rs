use crate::order::{OrderId, TradeId};

#[derive(Debug, Clone)]
pub struct Trade {
    pub id: TradeId,
    /// Volume in whole units
    pub size: i64,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub entry_bar: usize,
    pub exit_bar: Option<usize>,
    pub sl_order: Option<OrderId>,
    pub tp_order: Option<OrderId>,

    pub open_sl: Option<f64>,
    pub tag: Option<String>,

    /// Only finalized after trade is closed, need both leg
    pub commission: f64,
}

impl Trade {
    pub fn is_long(&self) -> bool {
        self.size > 0
    }

    pub fn is_short(&self) -> bool {
        self.size < 0
    }

    pub fn pl(&self, last_price: f64) -> f64 {
        let price = self.exit_price.unwrap_or(last_price);
        (self.size as f64) * (price - self.entry_price) - self.commission
    }

    pub fn value(&self, last_price: f64) -> f64 {
        let price = self.exit_price.unwrap_or(last_price);
        (self.size.unsigned_abs() as f64) * price
    }

    pub fn pl_pct(&self, last_price: f64) -> f64 {
        let price = self.exit_price.unwrap_or(last_price);
        let sign = if self.is_long() { 1.0 } else { -1.0 };
        let gross_pl_pct = sign * (price / self.entry_price - 1.0);
        let commission_pct = self.commission / (self.size.unsigned_abs() as f64 * self.entry_price);

        gross_pl_pct - commission_pct
    }
}

#[cfg(test)]
mod test {
    use super::*;

    macro_rules! assert_float_eq {
        ($left:expr, $right:expr, $epsilon:expr) => {
            let left_val = $left;
            let right_val = $right;
            let diff = (left_val - right_val).abs();
            assert!(
                diff < $epsilon,
                "assertion failed: `(left ≈ right)`\n  left: `{}`,\n right: `{}`,\n  diff: `{}`,\n max allowed diff: `{}`",
                left_val, right_val, diff, $epsilon
            );
        };
        ($left:expr, $right:expr) => {
            assert_float_eq!($left, $right, 1e-6);
        };
    }

    #[test]
    fn test_pl_long_trade() {
        let trade = Trade {
            id: 0,
            size: 10,
            entry_price: 100.0,
            exit_price: Some(110.0),
            entry_bar: 10,
            exit_bar: Some(15),
            sl_order: None,
            tp_order: None,
            open_sl: None,
            tag: None,
            commission: 10.0,
        };

        assert!(trade.is_long());
        assert!(!trade.is_short());

        // Price change: 10 units * ($110 - $100) = $100 gross gain.
        // Net P/L: $100 gain - $10 commission = $90.0
        assert_eq!(trade.pl(0.0), 90.0);

        // Position value = 10 * 100 = 1000
        // Gross return = (110 / 100) - 1 = +0.10 (+10%)
        // Commission pct = 10 / 1000 = 0.01 (1%)
        // Net return = 0.10 - 0.01 = 0.09 (+9%)
        assert_float_eq!(trade.pl_pct(0.0), 0.09);
    }

    #[test]
    fn test_pl_short_trade() {
        let trade = Trade {
            id: 1,
            size: -10,
            entry_price: 100.0,
            exit_price: Some(90.0),
            entry_bar: 10,
            exit_bar: Some(15),
            sl_order: None,
            tp_order: None,
            open_sl: None,
            tag: None,
            commission: 10.0,
        };

        assert!(trade.is_short());

        // Price change: -10 units * ($90 - $100) = $100 gross gain.
        // Net P/L: $100 gain - $10 commission = $90.0
        assert_eq!(trade.pl(0.0), 90.0);

        // Gross return = -1 * (90 / 100 - 1) = +0.10 (+10%)
        // Net return = 0.10 - 0.01 = 0.09 (+9%)
        assert_float_eq!(trade.pl_pct(0.0), 0.09);
    }
}

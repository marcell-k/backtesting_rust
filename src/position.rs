use crate::trade::Trade;

pub struct Position {
    size: f64,
    pl: f64,
    total_invested: f64,
}
impl Position {
    pub(crate) fn from_trades<'a>(
        trades: impl IntoIterator<Item = &'a Trade>,
        last_price: f64,
    ) -> Self {
        let mut size = 0.0;
        let mut pl = 0.0;
        let mut total_invested = 0.0;
        for trade in trades {
            size += trade.size as f64;
            pl += trade.pl(last_price);
            total_invested += trade.entry_price * (trade.size.unsigned_abs() as f64);
        }

        Self {
            size,
            pl,
            total_invested,
        }
    }

    pub fn size(&self) -> f64 {
        self.size
    }
    pub fn pl(&self) -> f64 {
        self.pl
    }
    pub fn total_invested(&self) -> f64 {
        self.total_invested
    }

    pub fn is_open(&self) -> bool {
        self.size != 0.0
    }
}

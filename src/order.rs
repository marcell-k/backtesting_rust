pub type OrderId = usize;
pub type TradeId = usize;

#[derive(Debug, Clone)]
pub struct Order {
    pub id: OrderId,
    /// positive = long, negative = short
    /// A magnitude in `(0,1)` is a fraction of equity, else absolute unit.
    pub size: f64,
    pub limit: Option<f64>,
    pub stop: Option<f64>,
    pub sl: Option<f64>,
    pub tp: Option<f64>,
    pub parent_trade: Option<TradeId>,
    pub tag: Option<String>,
}

impl Order {
    pub fn is_long(&self) -> bool {
        self.size > 0.0
    }

    pub fn is_short(&self) -> bool {
        self.size < 0.0
    }
}

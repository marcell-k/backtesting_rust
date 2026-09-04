use std::{
    ops::{Index, IndexMut},
    sync::Arc,
};

use crate::Trade;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrderId(pub(crate) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TradeId(pub(crate) usize);

impl Index<TradeId> for Vec<Trade> {
    type Output = Trade;

    fn index(&self, index: TradeId) -> &Trade {
        &self[index.0]
    }
}

impl IndexMut<TradeId> for Vec<Trade> {
    fn index_mut(&mut self, index: TradeId) -> &mut Self::Output {
        &mut self[index.0]
    }
}

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
    pub tag: Option<Arc<str>>,
}

impl Order {
    pub fn is_long(&self) -> bool {
        self.size > 0.0
    }

    pub fn is_short(&self) -> bool {
        self.size < 0.0
    }
}

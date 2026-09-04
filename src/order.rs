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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderSize {
    /// Signed fraction of equity, magnitude strictly in `(0, 1)`.
    Fraction(f64),
    /// Signed, exact number of whole units.
    Units(i64),
}

impl OrderSize {
    pub fn is_long(&self) -> bool {
        match self {
            OrderSize::Fraction(f) => *f > 0.0,
            OrderSize::Units(u) => *u > 0,
        }
    }

    pub fn is_short(&self) -> bool {
        match self {
            OrderSize::Fraction(f) => *f < 0.0,
            OrderSize::Units(u) => *u < 0,
        }
    }

    pub(crate) fn signed_f64(&self) -> f64 {
        match self {
            OrderSize::Fraction(f) => *f,
            OrderSize::Units(u) => *u as f64,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: OrderId,
    pub size: OrderSize,
    pub limit: Option<f64>,
    pub stop: Option<f64>,
    pub sl: Option<f64>,
    pub tp: Option<f64>,
    pub parent_trade: Option<TradeId>,
    pub tag: Option<Arc<str>>,
}

impl Order {
    pub fn is_long(&self) -> bool {
        self.size.is_long()
    }

    pub fn is_short(&self) -> bool {
        self.size.is_short()
    }
}

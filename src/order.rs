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

#[derive(Debug, Clone)]
pub struct OrderTable(Vec<Option<Order>>);
impl OrderTable {
    pub fn with_capacity(cap: usize) -> Self {
        Self(Vec::with_capacity(cap))
    }
    pub fn insert(&mut self, id: OrderId, order: Order) {
        if id.0 >= self.0.len() {
            self.0.resize_with(id.0 + 1, || None);
        }
        self.0[id.0] = Some(order)
    }

    pub fn remove(&mut self, id: OrderId) -> Option<Order> {
        self.0.get_mut(id.0).and_then(Option::take)
    }

    pub fn get(&self, id: OrderId) -> Option<&Order> {
        self.0.get(id.0).and_then(Option::as_ref)
    }

    pub fn get_mut(&mut self, id: OrderId) -> Option<&mut Order> {
        self.0.get_mut(id.0).and_then(Option::as_mut)
    }

    pub fn contains(&self, id: OrderId) -> bool {
        self.get(id).is_some()
    }
}

impl Index<OrderId> for OrderTable {
    type Output = Order;
    fn index(&self, index: OrderId) -> &Self::Output {
        self.get(index).expect("unknown order id")
    }
}

impl IndexMut<OrderId> for OrderTable {
    fn index_mut(&mut self, index: OrderId) -> &mut Self::Output {
        self.get_mut(index).expect("unknown order id")
    }
}

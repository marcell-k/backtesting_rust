use std::sync::Arc;

use crate::{
    broker::Broker,
    data::Data,
    error::BtResult,
    indicator::Indicator,
    order::{Order, OrderId, OrderSize as ResolvedOrderSize, TradeId},
    position::Position,
    trade::Trade,
};

pub trait Strategy {
    fn init(&mut self, _ctx: &mut Context) {}
    fn next(&mut self, ctx: &mut Context);
}

#[derive(Debug, Clone, Copy)]
pub enum OrderSize {
    /// shade very slightly under 100% to avoid float-rounding rejections
    All,
    /// fraction of equity (`0, 1`)
    Fraction(f64),
    /// whole number of units rounded
    Units(f64),
}

impl OrderSize {
    fn to_signed_size(self, is_buy: bool) -> BtResult<ResolvedOrderSize> {
        let resolved = match self {
            OrderSize::All => ResolvedOrderSize::Fraction(1.0 - f64::EPSILON),
            OrderSize::Fraction(f) => {
                if !(0.0 < f && f < 1.0) {
                    return Err(crate::error::BacktestError::InvalidParameter(
                        "size must be a fraction of equity".into(),
                    ));
                }
                ResolvedOrderSize::Fraction(f)
            }
            OrderSize::Units(u) => {
                if u.round() != u || u < 1.0 {
                    return Err(crate::error::BacktestError::InvalidParameter(
                        "size must be a whole positive number".into(),
                    ));
                }
                ResolvedOrderSize::Units(u as i64)
            }
        };
        Ok(match (resolved, is_buy) {
            (ResolvedOrderSize::Fraction(f), true) => ResolvedOrderSize::Fraction(f),
            (ResolvedOrderSize::Fraction(f), false) => ResolvedOrderSize::Fraction(-f),
            (ResolvedOrderSize::Units(u), true) => ResolvedOrderSize::Units(u),
            (ResolvedOrderSize::Units(u), false) => ResolvedOrderSize::Units(-u),
        })
    }
}

pub struct Context<'a> {
    pub data: &'a Data,
    pub(crate) broker: &'a mut Broker,
    pub(crate) indicators: &'a mut Vec<Indicator>,
    pub(crate) bar_index: usize,
}

impl<'a> Context<'a> {
    pub fn bar_index(&self) -> usize {
        self.bar_index
    }
    pub fn indicator(&mut self, ind: Indicator) -> usize {
        self.indicators.push(ind);
        self.indicators.len() - 1
    }
    pub fn indicator_value(&self, handle: usize) -> f64 {
        self.indicators[handle].value_at(self.bar_index)
    }
    pub fn indicator_series(&self, handle: usize) -> &[f64] {
        self.indicators[handle].as_of(self.bar_index)
    }

    pub fn buy(
        &mut self,
        size: OrderSize,
        limit: Option<f64>,
        stop: Option<f64>,
        sl: Option<f64>,
        tp: Option<f64>,
        tag: impl Into<Option<String>>,
    ) -> BtResult<OrderId> {
        let size = size.to_signed_size(true)?;
        self.broker.new_order(
            self.data,
            size,
            limit,
            stop,
            sl,
            tp,
            tag.into().map(Arc::from),
            None,
        )
    }
    pub fn sell(
        &mut self,
        size: OrderSize,
        limit: Option<f64>,
        stop: Option<f64>,
        sl: Option<f64>,
        tp: Option<f64>,
        tag: impl Into<Option<String>>,
    ) -> BtResult<OrderId> {
        let size = size.to_signed_size(false)?;
        self.broker.new_order(
            self.data,
            size,
            limit,
            stop,
            sl,
            tp,
            tag.into().map(Arc::from),
            None,
        )
    }

    pub fn equity(&self) -> f64 {
        self.broker.equity(self.data)
    }

    pub fn position(&self) -> Position {
        self.broker.position(self.data)
    }

    pub fn orders(&self) -> Vec<Order> {
        self.broker.orders().into_iter().cloned().collect()
    }

    pub fn trades(&self) -> Vec<Trade> {
        self.broker.trades().into_iter().cloned().collect()
    }

    pub fn closed_trades(&self) -> Vec<Trade> {
        self.broker.closed_trades().into_iter().cloned().collect()
    }

    /// Close `portion` of the current net position (`Position.close()`)
    pub fn close_position(&mut self, portion: f64) -> BtResult<()> {
        let trade_ids: Vec<TradeId> = self.broker.trades().iter().map(|t| t.id).collect();
        for tid in trade_ids {
            self.broker.request_trade_close(tid, portion)?;
        }
        Ok(())
    }

    /// Closes `portion` of a single trade (`Trade.close()`)
    pub fn close_trade(&mut self, trade_id: TradeId, portion: f64) -> BtResult<()> {
        self.broker
            .request_trade_close(trade_id, portion)
            .map(|_| ())
    }

    /// Sets, replaces, or (with `None`) cancels a trade's stop-loss (`Trade.sl` setter).
    pub fn set_trade_sl(&mut self, trade_id: TradeId, price: Option<f64>) -> BtResult<()> {
        self.broker.set_trade_sl(self.data, trade_id, price)
    }

    /// Sets, replaces, or (with `None`) cancels a trade's take-profit (`Trade.tp` setter).
    pub fn set_trade_tp(&mut self, trade_id: TradeId, price: Option<f64>) -> BtResult<()> {
        self.broker.set_trade_tp(self.data, trade_id, price)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_size_of_one_resolves_to_units_variant() {
        let resolved = OrderSize::Units(1.0).to_signed_size(true).unwrap();
        assert_eq!(resolved, ResolvedOrderSize::Units(1));
    }

    #[test]
    fn fraction_just_below_one_resolves_to_fraction_variant() {
        let resolved = OrderSize::Fraction(0.999999999)
            .to_signed_size(true)
            .unwrap();
        match resolved {
            ResolvedOrderSize::Fraction(f) => assert!((f - 0.999999999).abs() < 1e-12),
            ResolvedOrderSize::Units(_) => panic!("expected Fraction variant, got Units"),
        }
    }

    #[test]
    fn all_resolves_to_fraction_variant_not_units() {
        let resolved = OrderSize::All.to_signed_size(true).unwrap();
        assert!(matches!(resolved, ResolvedOrderSize::Fraction(_)));
    }

    #[test]
    fn sell_negates_resolved_size() {
        let resolved = OrderSize::Units(5.0).to_signed_size(false).unwrap();
        assert_eq!(resolved, ResolvedOrderSize::Units(-5));
    }
}

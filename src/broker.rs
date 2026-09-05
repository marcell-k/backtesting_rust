use std::sync::Arc;

use crate::{
    commission::Commission,
    data::{Data, Field},
    error::{BacktestError, BtResult},
    order::{Order, OrderId, OrderSize, OrderTable, TradeId},
    position::Position,
    trade::Trade,
};

#[derive(Clone, Debug)]
pub struct BrokerConfig {
    pub cash: f64,
    pub spread: f64,
    pub commission: Commission,
    /// margin ratio (`1/leverage`), in (`0, 1`]
    pub margin: f64,
    pub trade_on_close: bool,
    pub hedging: bool,
    pub exclusive_orders: bool,
}
impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            cash: 1e7,
            spread: 0.0,
            commission: Commission::default(),
            margin: 1.0,
            trade_on_close: true,
            hedging: false,
            exclusive_orders: false,
        }
    }
}

pub struct Broker {
    cash: f64,
    commission: Commission,
    spread: f64,
    leverage: f64,
    trade_on_close: bool,
    hedging: bool,
    exclusive_orders: bool,

    orders_by_id: OrderTable,
    /// Prcossening orders for pending orders. SL orders are inserted at the front so they're
    /// matched before other queued orders within the same bar.
    order_queue: Vec<OrderId>,
    /// Scratch buffer reused every bar for the process_orders() snapshot,
    /// so we don't heap-allocate a fresh Vec on every single bar.
    snapshot_buf: Vec<OrderId>,

    trades_by_id: Vec<Trade>,
    active_trade_ids: Vec<TradeId>,
    closed_trade_ids: Vec<TradeId>,

    equity_curve: Vec<f64>,

    next_order_id: OrderId,
    next_trade_id: TradeId,
    current_bar: usize,

    /// Non-fatal runtime warnings (insufficient margin, canceled orders,
    /// dubious same-bar SL/TP fills, ...), collected instead of printed so
    /// library consumers can decide what to do with them.
    pub warnings: Vec<String>,
}

impl Broker {
    pub fn new(config: BrokerConfig, n_bars: usize) -> BtResult<Self> {
        if config.cash <= 0.0 {
            return Err(BacktestError::InvalidParameter(format!(
                "cash should be > 0, got {}",
                config.cash
            )));
        }
        if !(0.0 < config.margin && config.margin <= 1.0) {
            return Err(BacktestError::InvalidParameter(format!(
                "margin should be between 0 and 1, got {}",
                config.margin
            )));
        }
        Ok(Self {
            cash: config.cash,
            commission: config.commission,
            spread: config.spread,
            leverage: 1.0 / config.margin,
            trade_on_close: config.trade_on_close,
            hedging: config.hedging,
            exclusive_orders: config.exclusive_orders,
            orders_by_id: OrderTable::with_capacity(n_bars),
            order_queue: Vec::new(),
            snapshot_buf: Vec::new(),
            trades_by_id: Vec::new(),
            active_trade_ids: Vec::new(),
            closed_trade_ids: Vec::new(),
            equity_curve: vec![f64::NAN; n_bars],
            next_order_id: OrderId(0),
            next_trade_id: TradeId(0),
            current_bar: 0,
            warnings: Vec::new(),
        })
    }

    // --- read-only views + take ---
    pub fn cash(&self) -> f64 {
        self.cash
    }
    pub fn orders(&self) -> impl Iterator<Item = &Order> + '_ {
        self.order_queue
            .iter()
            .map(move |&id| &self.orders_by_id[id])
    }
    pub fn trades(&self) -> impl DoubleEndedIterator<Item = &Trade> + '_ {
        self.active_trade_ids
            .iter()
            .map(move |&id| &self.trades_by_id[id])
    }
    pub fn closed_trades(&self) -> impl Iterator<Item = &Trade> + '_ {
        self.closed_trade_ids
            .iter()
            .map(move |&id| &self.trades_by_id[id])
    }
    pub fn take_closed_trades(&mut self) -> Vec<Trade> {
        let closed_ids: std::collections::HashSet<usize> =
            self.closed_trade_ids.iter().map(|id| id.0).collect();
        let all = std::mem::take(&mut self.trades_by_id);
        all.into_iter()
            .enumerate()
            .filter(|(i, _)| closed_ids.contains(i))
            .map(|(_, t)| t)
            .collect()
    }
    pub fn equity_curve(&self) -> &[f64] {
        &self.equity_curve
    }
    pub fn take_equity_curve(&mut self) -> Vec<f64> {
        std::mem::take(&mut self.equity_curve)
    }
    pub fn position(&self, data: &Data) -> Position {
        let last_price = self.last_price(data);
        Position::from_trades(self.trades(), last_price)
    }
    pub fn last_price(&self, data: &Data) -> f64 {
        data.at(crate::data::Field::Close, -1)
    }
    fn adjusted_price(&self, is_long: bool, data: &Data, price: Option<f64>) -> f64 {
        let spread = if is_long { self.spread } else { -self.spread };
        price.unwrap_or_else(|| self.last_price(data)) * (1.0 + spread)
    }
    pub fn equity(&self, data: &Data) -> f64 {
        let last = self.last_price(data);
        self.cash
            + self
                .active_trade_ids
                .iter()
                .map(|&id| self.trades_by_id[id].pl(last))
                .sum::<f64>()
    }

    pub fn margin_available(&self, data: &Data) -> f64 {
        let last = self.last_price(data);
        let margin_used: f64 = self
            .active_trade_ids
            .iter()
            .map(|&id| self.trades_by_id[id].value(last) / self.leverage)
            .sum();
        (self.equity(data) - margin_used).max(0.0)
    }

    fn order_is_contingent(&self, order_id: OrderId) -> bool {
        self.orders_by_id
            .get(order_id)
            .and_then(|o| o.parent_trade)
            .and_then(|tid| self.trades_by_id.get(tid.0))
            .map(|t| t.sl_order == Some(order_id) || t.tp_order == Some(order_id))
            .unwrap_or(false)
    }

    // --- order placement ---
    pub fn new_order(
        &mut self,
        data: &Data,
        size: OrderSize,
        limit: Option<f64>,
        stop: Option<f64>,
        sl: Option<f64>,
        tp: Option<f64>,
        tag: Option<Arc<str>>,
        trade: Option<TradeId>,
    ) -> BtResult<OrderId> {
        let is_zero = match size {
            OrderSize::Fraction(f) => f == 0.0,
            OrderSize::Units(u) => u == 0,
        };
        if is_zero {
            return Err(BacktestError::InvalidOrder("size must be nonzero".into()));
        }
        let is_long = size.is_long();
        let adjusted_price = self.adjusted_price(is_long, data, None);
        let ref_price = limit.or(stop).unwrap_or(adjusted_price);
        if is_long {
            if !(sl.unwrap_or(f64::NEG_INFINITY) < ref_price
                && ref_price < tp.unwrap_or(f64::INFINITY))
            {
                return Err(BacktestError::InvalidOrder(format!(
                    "Long orders require: SL ({sl:?}) < LIMIT ({ref_price}) < TP ({tp:?})"
                )));
            }
        } else if !(tp.unwrap_or(f64::NEG_INFINITY) < ref_price
            && ref_price < sl.unwrap_or(f64::INFINITY))
        {
            return Err(BacktestError::InvalidOrder(format!(
                "Short orders require: TP ({tp:?}) < LIMIT ({ref_price}) < SL ({sl:?})"
            )));
        }

        let id = self.alloc_order_id();
        let order = Order {
            id,
            size,
            limit,
            stop,
            sl,
            tp,
            parent_trade: trade,
            tag,
        };

        if trade.is_none() && self.exclusive_orders {
            // auto-close previous order/position
            let to_cancel: Vec<OrderId> = self
                .order_queue
                .iter()
                .copied()
                .filter(|&oid| !self.order_is_contingent(oid))
                .collect();
            for oid in to_cancel {
                self.cancel_order(oid);
            }
            let to_close: Vec<TradeId> = self.active_trade_ids.clone();
            for tid in to_close {
                self.request_trade_close(tid, 1.0)?;
            }
        }

        let is_sl_order = trade.is_some() && stop.is_some();
        if is_sl_order {
            self.orders_by_id.insert(id, order);
            self.order_queue.insert(0, id);
        } else {
            self.enqueue_order(order);
        }
        Ok(id)
    }

    fn alloc_order_id(&mut self) -> OrderId {
        let id = self.next_order_id;
        self.next_order_id.0 += 1;
        id
    }

    fn alloc_trade_id(&mut self) -> TradeId {
        let id = self.next_trade_id;
        self.next_trade_id.0 += 1;
        id
    }

    fn enqueue_order(&mut self, order: Order) -> OrderId {
        let id = order.id;
        self.orders_by_id.insert(id, order);
        self.order_queue.push(id);
        id
    }

    /// cancel a pending order
    pub fn cancel_order(&mut self, order_id: OrderId) {
        self.order_queue.retain(|&id| id != order_id);
        if let Some(order) = self.orders_by_id.remove(order_id)
            && let Some(tid) = order.parent_trade
            && let Some(trade) = self.trades_by_id.get_mut(tid.0)
        {
            if trade.sl_order == Some(order_id) {
                trade.sl_order = None;
            } else if trade.tp_order == Some(order_id) {
                trade.tp_order = None;
            }
        }
    }

    /// place a new order to close `portion` of `trade_id` at the next market price (`Trade.close`)
    pub fn request_trade_close(&mut self, trade_id: TradeId, portion: f64) -> BtResult<OrderId> {
        if !(0.0 < portion && portion <= 1.0) {
            return Err(BacktestError::InvalidParameter(format!(
                "portion must be between 0 and 1, got {portion}"
            )));
        }
        let trade = self
            .trades_by_id
            .get(trade_id.0)
            .ok_or_else(|| BacktestError::Other("unknown trade id".into()))?;
        let mag = 1i64.max((trade.size.unsigned_abs() as f64 * portion).round() as i64);
        let size = if trade.size > 0 { -mag } else { mag };
        let tag = trade.tag.clone();

        let id = self.alloc_order_id();
        let order = Order {
            id,
            size: OrderSize::Units(size),
            limit: None,
            stop: None,
            sl: None,
            tp: None,
            parent_trade: Some(trade_id),
            tag,
        };
        Ok(self.enqueue_order(order))
    }

    pub fn set_trade_sl(
        &mut self,
        data: &Data,
        trade_id: TradeId,
        price: Option<f64>,
    ) -> BtResult<()> {
        self.set_contingent(data, trade_id, price, true)
    }
    pub fn set_trade_tp(
        &mut self,
        data: &Data,
        trade_id: TradeId,
        price: Option<f64>,
    ) -> BtResult<()> {
        self.set_contingent(data, trade_id, price, false)
    }

    fn set_contingent(
        &mut self,
        data: &Data,
        trade_id: TradeId,
        price: Option<f64>,
        is_sl: bool,
    ) -> BtResult<()> {
        if let Some(p) = price
            && !(0.0 < p && p < f64::INFINITY)
        {
            return Err(BacktestError::InvalidParameter(format!(
                "Make sure 0 < price < inf! price: {p}"
            )));
        }
        let existing = {
            let t = self
                .trades_by_id
                .get(trade_id.0)
                .ok_or_else(|| BacktestError::Other("unknown trade id".into()))?;
            if is_sl { t.sl_order } else { t.tp_order }
        };
        if let Some(oid) = existing {
            self.cancel_order(oid);
        }
        if let Some(p) = price {
            let trade_size = self.trades_by_id[trade_id].size;
            let tag = self.trades_by_id[trade_id].tag.clone();
            let (limit, stop) = if is_sl {
                (None, Some(p))
            } else {
                (Some(p), None)
            };
            let order_id = self.new_order(
                data,
                OrderSize::Units(-trade_size),
                limit,
                stop,
                None,
                None,
                tag,
                Some(trade_id),
            )?;
            let t = self.trades_by_id.get_mut(trade_id.0).unwrap();
            if is_sl {
                t.sl_order = Some(order_id)
            } else {
                t.tp_order = Some(order_id)
            }
        } else if is_sl {
            self.trades_by_id.get_mut(trade_id.0).unwrap().sl_order = None;
        } else {
            self.trades_by_id.get_mut(trade_id.0).unwrap().tp_order = None;
        }
        Ok(())
    }

    /// --- per-bar simulation ---
    pub fn advance(&mut self, data: &Data, bar_index: usize) -> BtResult<()> {
        self.current_bar = bar_index;
        self.process_orders(data)?;

        let equity = self.equity(data);
        self.equity_curve[bar_index] = equity;

        if equity <= 0.0 {
            let last = data.at(Field::Close, -1);
            let ids: Vec<TradeId> = self.active_trade_ids.clone();
            for tid in ids {
                self.close_trade(tid, last, bar_index);
            }
            self.cash = 0.0;
            for v in &mut self.equity_curve[bar_index..] {
                *v = 0.0;
            }
            return Err(BacktestError::OutOfMoney);
        }
        Ok(())
    }

    fn process_orders(&mut self, data: &Data) -> BtResult<()> {
        loop {
            if self.order_queue.is_empty() {
                return Ok(());
            }
            let open = data.at(Field::Open, -1);
            let high = data.at(Field::High, -1);
            let low = data.at(Field::Low, -1);
            let mut reprocess_orders = false;

            let mut snapshot = std::mem::take(&mut self.snapshot_buf);
            snapshot.clear();
            snapshot.extend_from_slice(&self.order_queue);

            for &order_id in &snapshot {
                // The related SL/TP sibiling order may have already been removed by a prior iteration
                // of this same loop (e.g. hedged position)
                if !self.orders_by_id.contains(order_id) {
                    continue;
                }
                let mut order = self.orders_by_id[order_id].clone();

                // -- stop trigger ? --
                let stop_price = order.stop;
                if let Some(sp) = stop_price {
                    let is_stop_hit = if order.is_long() {
                        high >= sp
                    } else {
                        low <= sp
                    };
                    if !is_stop_hit {
                        continue;
                    }
                    // A triggered stop order becomes a market/limit order
                    order.stop = None;
                    self.orders_by_id.insert(order_id, order.clone());
                }
                let is_contingent = self.order_is_contingent(order_id);

                // -- determine fill price --
                let price: f64;
                if let Some(limit) = order.limit {
                    let is_limit_hit = if order.is_long() {
                        low <= limit
                    } else {
                        high >= limit
                    };
                    let is_limit_hit_before_stop = is_limit_hit
                        && if order.is_long() {
                            limit <= stop_price.unwrap_or(f64::NEG_INFINITY)
                        } else {
                            limit >= stop_price.unwrap_or(f64::INFINITY)
                        };
                    if !is_limit_hit || is_limit_hit_before_stop {
                        continue;
                    }
                    price = if order.is_long() {
                        stop_price.unwrap_or(open).min(limit)
                    } else {
                        stop_price.unwrap_or(open).max(limit)
                    };
                } else {
                    let prev_close = data.at(Field::Close, -2);
                    let mut p = if self.trade_on_close && !is_contingent && !prev_close.is_nan() {
                        prev_close
                    } else {
                        open
                    };
                    if let Some(sp) = stop_price {
                        p = if order.is_long() {
                            p.max(sp)
                        } else {
                            p.min(sp)
                        }
                    }
                    price = p;
                }

                let is_market_order = order.limit.is_none() && stop_price.is_none();
                let time_index = if is_market_order && self.trade_on_close && !is_contingent {
                    self.current_bar.saturating_sub(1)
                } else {
                    self.current_bar
                };

                // -- contingent order: closes/reduces on existing trade --
                if let Some(trade_id) = order.parent_trade {
                    let prev_size = match self.trades_by_id.get(trade_id.0) {
                        Some(t) => t.size,
                        None => {
                            self.remove_order(order_id);
                            continue;
                        }
                    };
                    let order_units = match order.size {
                        OrderSize::Units(u) => u,
                        OrderSize::Fraction(_) => {
                            unreachable!("contingent (trade-closing) orders are always whole units")
                        }
                    };
                    let mag = prev_size.unsigned_abs().min(order_units.unsigned_abs()) as i64;
                    let close_size = if order_units >= 0 { mag } else { -mag };

                    if self.active_trade_ids.contains(&trade_id) {
                        self.reduce_trade(trade_id, price, close_size, time_index);

                        // Restore the SL order's stop price after a same-price
                        // trigger, so the surviving (possibly resized) order
                        // still behaves as a stop order on subsequent bars.
                        if let Some(sp) = stop_price
                            && price == sp
                            && let Some(trade) = self.trades_by_id.get(trade_id.0)
                            && let Some(sl_oid) = trade.sl_order
                            && let Some(order) = self.orders_by_id.get_mut(sl_oid)
                        {
                            order.stop = Some(sp);
                        }
                    }

                    let is_bracket = self
                        .trades_by_id
                        .get(trade_id.0)
                        .map(|t| t.sl_order == Some(order_id) || t.tp_order == Some(order_id))
                        .unwrap_or(false);
                    if !is_bracket {
                        self.remove_order(order_id);
                    }
                    continue;
                }

                // -- standalone order: opens a new trade --
                let opened = self.fill_standalone_order(&order, price, time_index, data)?;
                if opened && (order.sl.is_some() || order.tp.is_some()) {
                    let tp_same_bar_safe = stop_price.is_some()
                        && order.limit.is_none()
                        && order.tp.is_some()
                        && (order.is_long()
                            && order.tp.unwrap() <= high
                            && order.sl.unwrap_or(f64::NEG_INFINITY) > high);

                    if is_market_order || tp_same_bar_safe {
                        reprocess_orders = true;
                    } else if (low..=high).contains(&order.sl.unwrap_or(f64::NEG_INFINITY))
                        || (low..=high).contains(&order.tp.unwrap_or(f64::NEG_INFINITY))
                    {
                        self.warnings.push("A contingent SL/TP order would execute in the same bar its parent stop/limit order was turned into a trade. Since we can't assert the precise intra-candle price movement, the affected SL/TP order will instead be executed on the next  (matching) price/bar, making the result (of this trade) somewhat dubious.".to_string()
                        );
                    }
                }
                self.remove_order(order_id);
            }
            snapshot.clear();
            self.snapshot_buf = snapshot;
            if !reprocess_orders {
                return Ok(());
            }
            // otherwise loop again on the same bar instead of recursing
        }
    }

    fn remove_order(&mut self, order_id: OrderId) {
        self.order_queue.retain(|id| *id != order_id);
        self.orders_by_id.remove(order_id);
    }

    fn reduce_trade(&mut self, trade_id: TradeId, price: f64, size: i64, time_index: usize) {
        let (prev_size, entry_price, entry_bar, tag, sl_order, tp_order) = {
            let t = &self.trades_by_id[trade_id];
            (
                t.size,
                t.entry_price,
                t.entry_bar,
                t.tag.clone(),
                t.sl_order,
                t.tp_order,
            )
        };
        assert!(
            prev_size * size < 0,
            "reduce_trade: size {size} must oppose existing trade size {prev_size}"
        );
        assert!(
            prev_size.unsigned_abs() >= size.unsigned_abs(),
            "reduce_trade: closing size {size} exceeds trade size {prev_size} \
             (trade_id={}) — indicates an upstream sizing/rounding bug",
            trade_id.0
        );
        let size_left = prev_size + size;

        let close_trade_id = if size_left == 0 {
            trade_id
        } else {
            self.trades_by_id.get_mut(trade_id.0).unwrap().size = size_left;
            if let Some(oid) = sl_order
                && let Some(o) = self.orders_by_id.get_mut(oid)
            {
                o.size = OrderSize::Units(-size_left);
            }
            if let Some(oid) = tp_order
                && let Some(o) = self.orders_by_id.get_mut(oid)
            {
                o.size = OrderSize::Units(-size_left);
            }

            let new_id = self.alloc_trade_id();
            self.trades_by_id.push(Trade {
                id: new_id,
                size: -size,
                entry_price,
                exit_price: None,
                entry_bar,
                exit_bar: None,
                sl_order: None,
                tp_order: None,
                open_sl: None,
                tag,
                commission: 0.0,
            });
            self.active_trade_ids.push(new_id);
            new_id
        };
        self.close_trade(close_trade_id, price, time_index);
    }

    fn close_trade(&mut self, trade_id: TradeId, price: f64, time_index: usize) {
        self.active_trade_ids.retain(|id| *id != trade_id);

        let (sl_order, tp_order, size, entry_price) = {
            let t = &self.trades_by_id[trade_id];
            (t.sl_order, t.tp_order, t.size, t.entry_price)
        };
        if let Some(oid) = sl_order {
            self.remove_order(oid);
        }
        if let Some(oid) = tp_order {
            self.remove_order(oid);
        }

        let commission_exit = self.commission.compute(size as f64, price);
        let commission_entry = self.commission.compute(size as f64, entry_price);

        let t = self.trades_by_id.get_mut(trade_id.0).unwrap();
        t.exit_price = Some(price);
        t.exit_bar = Some(time_index);
        let pl = t.pl(price);
        t.commission = commission_entry + commission_exit;

        self.cash += pl - commission_exit;
        self.closed_trade_ids.push(trade_id);
    }

    fn fill_standalone_order(
        &mut self,
        order: &Order,
        price: f64,
        time_index: usize,
        data: &Data,
    ) -> BtResult<bool> {
        let adjusted_price = self.adjusted_price(order.is_long(), data, Some(price));
        let raw_size = order.size.signed_f64();
        let commission_per_unit = self.commission.compute(raw_size, price) / raw_size.abs();
        let adjusted_price_plus_commission = adjusted_price + commission_per_unit;

        let mut need_size: i64 = match order.size {
            OrderSize::Units(u) => u,
            OrderSize::Fraction(f) => {
                let margin_available = self.margin_available(data);
                let units = ((margin_available * self.leverage * f.abs())
                    / adjusted_price_plus_commission)
                    .floor();
                let signed_units = if f >= 0.0 { units } else { -units };
                if signed_units == 0.0 {
                    self.warnings.push(format!(
                        "time={}: Broker canceled the relative-sized order due to \
                         insufficient margin (equity={:.2}, margin_available={:.2}).",
                        self.current_bar,
                        self.equity(data),
                        margin_available
                    ));
                    return Ok(false);
                }
                signed_units as i64
            }
        };

        if !self.hedging {
            let opposite_trades: Vec<TradeId> = self
                .active_trade_ids
                .iter()
                .copied()
                .filter(|&tid| self.trades_by_id[tid].is_long() != order.is_long())
                .collect();
            for trade_id in opposite_trades {
                let trade_size = self.trades_by_id[trade_id].size;
                if need_size.unsigned_abs() >= trade_size.unsigned_abs() {
                    self.close_trade(trade_id, price, time_index);
                    need_size += trade_size;
                } else {
                    self.reduce_trade(trade_id, price, need_size, time_index);
                    need_size = 0;
                }
                if need_size == 0 {
                    break;
                }
            }
        }

        if need_size.unsigned_abs() as f64 * adjusted_price_plus_commission
            > self.margin_available(data) * self.leverage
        {
            self.warnings.push(format!(
                "time={}: Broker canceled the order due to insufficient margin \
                 (equity={:.2}, margin_available={:.2}).",
                self.current_bar,
                self.equity(data),
                self.margin_available(data)
            ));
            return Ok(false);
        }

        if need_size != 0 {
            self.open_trade(
                adjusted_price,
                need_size,
                order.sl,
                order.tp,
                time_index,
                order.tag.clone(),
                data,
            )?;
            return Ok(true);
        }
        Ok(false)
    }

    fn open_trade(
        &mut self,
        price: f64,
        size: i64,
        sl: Option<f64>,
        tp: Option<f64>,
        time_index: usize,
        tag: Option<Arc<str>>,
        data: &Data,
    ) -> BtResult<TradeId> {
        let id = self.alloc_trade_id();
        self.trades_by_id.push(Trade {
            id,
            size,
            entry_price: price,
            exit_price: None,
            entry_bar: time_index,
            exit_bar: None,
            sl_order: None,
            tp_order: None,
            open_sl: sl,
            tag,
            commission: 0.0,
        });

        self.active_trade_ids.push(id);
        self.cash -= self.commission.compute(size as f64, price);

        if let Some(tp_price) = tp {
            self.set_trade_tp(data, id, Some(tp_price))?;
        }
        if let Some(sl_price) = sl {
            self.set_trade_sl(data, id, Some(sl_price))?;
        }
        Ok(id)
    }
}

#[cfg(test)]
mod broker_fix_tests {
    use super::*;
    use crate::data::Data;
    use chrono::NaiveDate;

    fn flat_data(price: f64, n: usize) -> Data {
        let start = NaiveDate::from_ymd_opt(2024, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let index = (0..n)
            .map(|i| start + chrono::Duration::days(i as i64))
            .collect();
        Data::new(
            index,
            vec![price; n],
            vec![price; n],
            vec![price; n],
            vec![price; n],
            vec![1000.0; n],
        )
        .unwrap()
    }

    #[test]
    fn canceled_order_without_tp_does_not_open_a_trade() {
        let mut data = flat_data(100.0, 5);
        let config = BrokerConfig {
            cash: 100.0,
            margin: 1.0,
            ..Default::default()
        };
        let mut broker = Broker::new(config, 5).unwrap();
        data.set_length(2);

        broker
            .new_order(
                &data,
                OrderSize::Units(1_000_000),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();

        broker.advance(&data, 1).unwrap();

        assert!(
            broker.trades().next().is_none(),
            "an order that failed margin checks must not open a trade"
        );
        assert!(
            !broker.warnings.is_empty(),
            "a margin-rejection warning should have been recorded"
        );
    }

    #[test]
    fn fractional_size_rounds_correctly_at_fp_boundary() {
        let price = 99.999999999999_f64;
        let mut data = flat_data(price, 2);
        let config = BrokerConfig {
            cash: price * 1000.0,
            margin: 1.0,
            commission: Commission::relative(0.0),
            ..Default::default()
        };
        let mut broker = Broker::new(config, 2).unwrap();
        data.set_length(1);

        broker
            .new_order(
                &data,
                OrderSize::Fraction(0.999999999),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();
        broker.advance(&data, 0).unwrap();

        let trades: Vec<_> = broker.trades().collect();
        assert_eq!(trades.len(), 1, "expected exactly one trade to open");
        assert!(
            trades[0].size >= 999,
            "unit count should not be truncated by fp rounding noise, got {}",
            trades[0].size
        );
    }
}

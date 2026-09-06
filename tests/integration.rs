use backtesting::{
    Backtest, BrokerConfig, Commission, Context, Data, OrderSize, Strategy, TradeId,
};
use chrono::NaiveDate;
use std::sync::Arc;

fn bars(rows: &[(f64, f64, f64, f64)]) -> Data {
    let start = NaiveDate::from_ymd_opt(2024, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let n = rows.len();
    let index = (0..n)
        .map(|i| start + chrono::Duration::days(i as i64))
        .collect();
    let open = rows.iter().map(|r| r.0).collect();
    let high = rows.iter().map(|r| r.1).collect();
    let low = rows.iter().map(|r| r.2).collect();
    let close = rows.iter().map(|r| r.3).collect();
    let volume = vec![1000.0; n];
    Data::new(index, open, high, low, close, volume).unwrap()
}

/// Buys once (with the given SL/TP) on `buy_at_bar`, then never trades again.
struct BuyOnceWithBracket {
    buy_at_bar: usize,
    sl: Option<f64>,
    tp: Option<f64>,
    bought: bool,
}

impl Strategy for BuyOnceWithBracket {
    fn init(&mut self, _ctx: &mut Context) {}

    fn next(&mut self, ctx: &mut Context) {
        if !self.bought && ctx.bar_index() == self.buy_at_bar {
            ctx.buy(OrderSize::All, None, None, self.sl, self.tp, None)
                .unwrap();
            self.bought = true;
        }
    }
}

/// Market orders fill on the *next* bar's open, and TP/SL bracket orders
/// resolve against subsequent bars' High/Low -- this checks both, plus
/// that the fill price is exactly the touched TP limit (not e.g. the
/// bar's High).
#[test]
fn take_profit_fills_at_the_limit_price() {
    let data = bars(&[
        (100.0, 102.0, 98.0, 100.0),  // 0
        (100.0, 102.0, 98.0, 101.0),  // 1
        (101.0, 103.0, 99.0, 102.0),  // 2 <- strategy buys here
        (102.0, 104.0, 101.0, 103.0), // 3 <- fills at open=102; TP=110, SL=95
        (103.0, 105.0, 102.0, 104.0), // 4
        (104.0, 108.0, 103.0, 107.0), // 5
        (107.0, 115.0, 106.0, 112.0), // 6 <- High touches/exceeds TP=110
        (112.0, 113.0, 111.0, 112.0), // 7
        (112.0, 113.0, 111.0, 112.0), // 8
        (112.0, 113.0, 111.0, 112.0), // 9
        (112.0, 113.0, 111.0, 112.0), // 10
        (112.0, 113.0, 111.0, 112.0), // 11
    ]);

    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 10_000.0,
            commission: Commission::relative(0.0),
            trade_on_close: false,
            ..Default::default()
        },
    );
    let strat = BuyOnceWithBracket {
        buy_at_bar: 2,
        sl: Some(95.0),
        tp: Some(110.0),
        bought: false,
    };
    let result = bt.run(strat).unwrap();

    assert_eq!(
        result.closed_trades.len(),
        1,
        "expected exactly one closed trade"
    );
    let t = &result.closed_trades[0];
    assert_eq!(t.entry_bar, 3);
    assert!(
        (t.entry_price - 102.0).abs() < 1e-9,
        "entry should fill at bar 3's open, got {}",
        t.entry_price
    );
    assert_eq!(t.exit_bar, Some(6));
    assert!(
        (t.exit_price.unwrap() - 110.0).abs() < 1e-9,
        "exit should fill exactly at the TP limit, got {:?}",
        t.exit_price
    );
    assert!(t.is_long());
}

#[test]
fn stop_loss_fills_when_price_dips() {
    let data = bars(&[
        (100.0, 102.0, 98.0, 100.0),  // 0
        (100.0, 102.0, 98.0, 101.0),  // 1
        (101.0, 103.0, 99.0, 102.0),  // 2 <- strategy buys here
        (102.0, 104.0, 101.0, 103.0), // 3 <- fills at open=102; TP=110, SL=95
        (103.0, 105.0, 102.0, 104.0), // 4
        (104.0, 106.0, 96.0, 97.0),   // 5
        (97.0, 98.0, 90.0, 92.0),     // 6 <- Low breaches SL=95
        (92.0, 93.0, 91.0, 92.0),     // 7
        (92.0, 93.0, 91.0, 92.0),     // 8
        (92.0, 93.0, 91.0, 92.0),     // 9
        (92.0, 93.0, 91.0, 92.0),     // 10
        (92.0, 93.0, 91.0, 92.0),     // 11
    ]);

    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 10_000.0,
            commission: Commission::relative(0.0),
            ..Default::default()
        },
    );
    let strat = BuyOnceWithBracket {
        buy_at_bar: 2,
        sl: Some(95.0),
        tp: Some(110.0),
        bought: false,
    };
    let result = bt.run(strat).unwrap();

    assert_eq!(result.closed_trades.len(), 1);
    let t = &result.closed_trades[0];
    assert_eq!(t.exit_bar, Some(6));
    assert!(
        (t.exit_price.unwrap() - 95.0).abs() < 1e-9,
        "exit should fill exactly at the SL stop, got {:?}",
        t.exit_price
    );
    assert!(t.pl(0.0) < 0.0, "a stopped-out long should show a loss");
}

#[test]
fn open_trade_is_finalized_at_the_end() {
    let data = bars(&[
        (100.0, 101.0, 99.0, 100.0),
        (100.0, 101.0, 99.0, 101.0),
        (101.0, 102.0, 100.0, 102.0), // buy here
        (102.0, 103.0, 101.0, 103.0),
        (103.0, 104.0, 102.0, 104.0),
        (104.0, 105.0, 103.0, 105.0),
    ]);
    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 10_000.0,
            ..Default::default()
        },
    );
    let strat = BuyOnceWithBracket {
        buy_at_bar: 2,
        sl: None,
        tp: None,
        bought: false,
    };
    let result = bt.run(strat).unwrap();

    assert_eq!(
        result.closed_trades.len(),
        1,
        "the still-open trade should be force-closed"
    );
    assert!(result.closed_trades[0].exit_price.is_some());
}

#[test]
fn insufficient_margin_rejects_absolute_size_order() {
    struct BuyHugeAbsoluteSize;
    impl Strategy for BuyHugeAbsoluteSize {
        fn init(&mut self, _ctx: &mut Context) {}
        fn next(&mut self, ctx: &mut Context) {
            if ctx.bar_index() == 1 {
                // Try to buy far more whole units than the $1,000 cash could ever cover.
                ctx.buy(OrderSize::Units(1_000_000.0), None, None, None, None, None)
                    .unwrap();
            }
        }
    }

    let data = bars(&[
        (100.0, 101.0, 99.0, 100.0),
        (100.0, 101.0, 99.0, 101.0),
        (101.0, 102.0, 100.0, 102.0),
        (102.0, 103.0, 101.0, 103.0),
    ]);
    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 1_000.0,
            ..Default::default()
        },
    );
    let result = bt.run(BuyHugeAbsoluteSize).unwrap();

    assert_eq!(
        result.closed_trades.len(),
        0,
        "the oversized order should have been canceled, not filled"
    );
    assert!(
        !result.warnings.is_empty(),
        "a margin-rejection warning should have been recorded"
    );
}

#[test]
fn tag_propagates_from_order_to_trade() {
    struct BuyOnceWithTag {
        buy_at_bar: usize,
        bought: bool,
    }
    impl Strategy for BuyOnceWithTag {
        fn init(&mut self, _ctx: &mut Context) {}
        fn next(&mut self, ctx: &mut Context) {
            if !self.bought && ctx.bar_index() == self.buy_at_bar {
                ctx.buy(
                    OrderSize::All,
                    None,
                    None,
                    None,
                    None,
                    Some("entry_signal".to_string()),
                )
                .unwrap();
                self.bought = true;
            }
        }
    }

    let data = bars(&[
        (100.0, 101.0, 99.0, 100.0),
        (100.0, 101.0, 99.0, 101.0),
        (101.0, 102.0, 100.0, 102.0), // buy here
        (102.0, 103.0, 101.0, 103.0),
        (103.0, 104.0, 102.0, 104.0),
        (104.0, 105.0, 103.0, 105.0),
    ]);
    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 10_000.0,
            ..Default::default()
        },
    );
    let strat = BuyOnceWithTag {
        buy_at_bar: 2,
        bought: false,
    };
    let result = bt.run(strat).unwrap();

    assert_eq!(result.closed_trades.len(), 1);
    let t = &result.closed_trades[0];
    assert_eq!(
        t.tag.as_deref(),
        Some("entry_signal"),
        "tag should survive from order placement through to the closed trade"
    );
}

#[test]
fn no_tag_yields_none() {
    struct BuyOnceNoTag {
        buy_at_bar: usize,
        bought: bool,
    }
    impl Strategy for BuyOnceNoTag {
        fn init(&mut self, _ctx: &mut Context) {}
        fn next(&mut self, ctx: &mut Context) {
            if !self.bought && ctx.bar_index() == self.buy_at_bar {
                ctx.buy(OrderSize::All, None, None, None, None, None)
                    .unwrap();
                self.bought = true;
            }
        }
    }

    let data = bars(&[
        (100.0, 101.0, 99.0, 100.0),
        (100.0, 101.0, 99.0, 101.0),
        (101.0, 102.0, 100.0, 102.0),
        (102.0, 103.0, 101.0, 103.0),
    ]);
    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 10_000.0,
            ..Default::default()
        },
    );
    let strat = BuyOnceNoTag {
        buy_at_bar: 2,
        bought: false,
    };
    let result = bt.run(strat).unwrap();

    assert_eq!(result.closed_trades.len(), 1);
    assert!(
        result.closed_trades[0].tag.is_none(),
        "an order placed without a tag should leave the trade untagged"
    );
}

#[test]
fn tag_survives_partial_close_and_shares_allocation() {
    struct BuyThenPartialClose {
        bought: bool,
        closed_half: bool,
    }
    impl Strategy for BuyThenPartialClose {
        fn init(&mut self, _ctx: &mut Context) {}
        fn next(&mut self, ctx: &mut Context) {
            if !self.bought && ctx.bar_index() == 1 {
                ctx.buy(
                    OrderSize::Units(10.0),
                    None,
                    None,
                    None,
                    None,
                    Some("swing_long".to_string()),
                )
                .unwrap();
                self.bought = true;
            } else if self.bought && !self.closed_half && ctx.bar_index() == 3 {
                ctx.close_position(0.5).unwrap();
                self.closed_half = true;
            }
        }
    }

    let data = bars(&[
        (100.0, 101.0, 99.0, 100.0),
        (100.0, 101.0, 99.0, 101.0), // buy here
        (101.0, 102.0, 100.0, 102.0),
        (102.0, 103.0, 101.0, 103.0), // close half here
        (103.0, 104.0, 102.0, 104.0),
        (104.0, 105.0, 103.0, 105.0),
    ]);
    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 10_000.0,
            commission: Commission::relative(0.0),
            ..Default::default()
        },
    );
    let strat = BuyThenPartialClose {
        bought: false,
        closed_half: false,
    };
    let result = bt.run(strat).unwrap();

    assert_eq!(
        result.closed_trades.len(),
        2,
        "expected the partial close plus the finalized remainder"
    );

    for t in &result.closed_trades {
        assert_eq!(
            t.tag.as_deref(),
            Some("swing_long"),
            "tag should propagate to both the closed chunk and the split-off remainder"
        );
    }

    let a = result.closed_trades[0].tag.as_ref().unwrap();
    let b = result.closed_trades[1].tag.as_ref().unwrap();
    assert!(
        Arc::ptr_eq(a, b),
        "tag should be shared via Arc, not re-allocated per trade"
    );
}

#[test]
fn hedging_keeps_long_and_short_trades_independent() {
    struct BuyThenSellHedged {
        bought: bool,
        sold: bool,
    }
    impl Strategy for BuyThenSellHedged {
        fn init(&mut self, _ctx: &mut Context) {}
        fn next(&mut self, ctx: &mut Context) {
            if !self.bought && ctx.bar_index() == 1 {
                ctx.buy(OrderSize::Units(5.0), None, None, None, None, None)
                    .unwrap();
                self.bought = true;
            } else if self.bought && !self.sold && ctx.bar_index() == 3 {
                ctx.sell(OrderSize::Units(3.0), None, None, None, None, None)
                    .unwrap();
                self.sold = true;
            }
        }
    }

    let data = bars(&[
        (100.0, 101.0, 99.0, 100.0),
        (100.0, 101.0, 99.0, 101.0),  // buy here
        (101.0, 102.0, 100.0, 102.0), // long fills at this bar's open (101.0)
        (102.0, 103.0, 101.0, 103.0), // sell here
        (103.0, 104.0, 102.0, 104.0), // short fills at this bar's open (103.0)
        (104.0, 105.0, 103.0, 105.0),
    ]);
    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 10_000.0,
            commission: Commission::relative(0.0),
            trade_on_close: false,
            hedging: true,
            ..Default::default()
        },
    );
    let strat = BuyThenSellHedged {
        bought: false,
        sold: false,
    };
    let result = bt.run(strat).unwrap();

    assert_eq!(
        result.closed_trades.len(),
        2,
        "hedging should open a separate trade instead of reducing the existing one"
    );
    let long_trade = result
        .closed_trades
        .iter()
        .find(|t| t.is_long())
        .expect("expected a long trade");
    let short_trade = result
        .closed_trades
        .iter()
        .find(|t| t.is_short())
        .expect("expected a short trade");

    assert_eq!(long_trade.size, 5);
    assert_eq!(long_trade.entry_bar, 2);
    assert!((long_trade.entry_price - 101.0).abs() < 1e-9);

    assert_eq!(short_trade.size, -3);
    assert_eq!(
        short_trade.entry_bar, 4,
        "a hedged short must be its own entry, not a split of the long trade"
    );
    assert!(
        (short_trade.entry_price - 103.0).abs() < 1e-9,
        "a hedged short's entry price must come from its own fill, not the long trade's"
    );
}

#[test]
fn exclusive_orders_auto_closes_before_opening_a_new_trade() {
    struct BuyTwice {
        bought_first: bool,
        bought_second: bool,
    }
    impl Strategy for BuyTwice {
        fn init(&mut self, _ctx: &mut Context) {}
        fn next(&mut self, ctx: &mut Context) {
            if !self.bought_first && ctx.bar_index() == 1 {
                ctx.buy(OrderSize::Units(5.0), None, None, None, None, None)
                    .unwrap();
                self.bought_first = true;
            } else if self.bought_first && !self.bought_second && ctx.bar_index() == 4 {
                // No explicit close first -- exclusive_orders should auto-close
                // the existing position before opening this one.
                ctx.buy(OrderSize::Units(5.0), None, None, None, None, None)
                    .unwrap();
                self.bought_second = true;
            }
        }
    }

    let data = bars(&[
        (100.0, 101.0, 99.0, 100.0),
        (100.0, 101.0, 99.0, 101.0),  // buy #1 here
        (101.0, 102.0, 100.0, 102.0), // entry #1 fills
        (102.0, 103.0, 101.0, 103.0),
        (103.0, 104.0, 102.0, 104.0), // buy #2 here -> auto-closes #1
        (104.0, 105.0, 103.0, 105.0), // auto-close #1 + entry #2 fill here
        (105.0, 106.0, 104.0, 106.0),
        (106.0, 107.0, 105.0, 107.0),
    ]);
    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 10_000.0,
            commission: Commission::relative(0.0),
            trade_on_close: false,
            exclusive_orders: true,
            ..Default::default()
        },
    );
    let strat = BuyTwice {
        bought_first: false,
        bought_second: false,
    };
    let result = bt.run(strat).unwrap();

    assert_eq!(result.closed_trades.len(), 2);
    let first = &result.closed_trades[0];
    let second = &result.closed_trades[1];

    assert_eq!(first.entry_bar, 2);
    assert_eq!(
        first.exit_bar,
        Some(5),
        "the first trade should be force-closed when the second order is placed"
    );
    assert_eq!(
        second.entry_bar, 5,
        "the new trade should open the same bar the old one was auto-closed"
    );
    assert!(
        second.exit_bar.unwrap() > 5,
        "the second trade should only close via finalize, not immediately again"
    );
}

#[test]
fn sl_replaced_twice_uses_the_latest_price() {
    struct BuyThenTightenSlTwice {
        bought: bool,
        tightened_once: bool,
        tightened_twice: bool,
        trade_id: Option<TradeId>,
    }
    impl Strategy for BuyThenTightenSlTwice {
        fn init(&mut self, _ctx: &mut Context) {}
        fn next(&mut self, ctx: &mut Context) {
            if !self.bought && ctx.bar_index() == 2 {
                ctx.buy(
                    OrderSize::Units(1.0),
                    None,
                    None,
                    Some(90.0),
                    Some(120.0),
                    None,
                )
                .unwrap();
                self.bought = true;
            } else if self.bought && !self.tightened_once && ctx.bar_index() == 4 {
                self.trade_id = Some(ctx.trades()[0].id);
                ctx.set_trade_sl(self.trade_id.unwrap(), Some(95.0))
                    .unwrap();
                self.tightened_once = true;
            } else if self.tightened_once && !self.tightened_twice && ctx.bar_index() == 5 {
                ctx.set_trade_sl(self.trade_id.unwrap(), Some(98.0))
                    .unwrap();
                self.tightened_twice = true;
            }
        }
    }

    let data = bars(&[
        (100.0, 101.0, 99.0, 100.0),
        (100.0, 101.0, 99.0, 101.0),
        (101.0, 102.0, 100.0, 102.0), // buy here, sl=90, tp=120
        (102.0, 103.0, 101.0, 103.0), // entry fills at open=102
        (103.0, 104.0, 102.0, 104.0), // tighten sl -> 95 here
        (104.0, 105.0, 103.0, 105.0), // tighten sl -> 98 here
        (105.0, 106.0, 96.0, 97.0),   // Low breaches 98, not 90 or 95
        (97.0, 98.0, 96.0, 97.0),
    ]);
    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 10_000.0,
            commission: Commission::relative(0.0),
            trade_on_close: false,
            ..Default::default()
        },
    );
    let strat = BuyThenTightenSlTwice {
        bought: false,
        tightened_once: false,
        tightened_twice: false,
        trade_id: None,
    };
    let result = bt.run(strat).unwrap();

    assert_eq!(result.closed_trades.len(), 1);
    let t = &result.closed_trades[0];
    assert_eq!(t.exit_bar, Some(6));
    assert!(
        (t.exit_price.unwrap() - 98.0).abs() < 1e-9,
        "should exit at the most recently set SL (98), not an earlier one, got {:?}",
        t.exit_price
    );
}

#[test]
fn tp_replaced_twice_uses_the_latest_price() {
    struct BuyThenTightenTpTwice {
        bought: bool,
        tightened_once: bool,
        tightened_twice: bool,
        trade_id: Option<TradeId>,
    }
    impl Strategy for BuyThenTightenTpTwice {
        fn init(&mut self, _ctx: &mut Context) {}
        fn next(&mut self, ctx: &mut Context) {
            if !self.bought && ctx.bar_index() == 2 {
                ctx.buy(OrderSize::Units(1.0), None, None, None, Some(120.0), None)
                    .unwrap();
                self.bought = true;
            } else if self.bought && !self.tightened_once && ctx.bar_index() == 4 {
                self.trade_id = Some(ctx.trades()[0].id);
                ctx.set_trade_tp(self.trade_id.unwrap(), Some(110.0))
                    .unwrap();
                self.tightened_once = true;
            } else if self.tightened_once && !self.tightened_twice && ctx.bar_index() == 5 {
                ctx.set_trade_tp(self.trade_id.unwrap(), Some(108.0))
                    .unwrap();
                self.tightened_twice = true;
            }
        }
    }

    let data = bars(&[
        (100.0, 101.0, 99.0, 100.0),
        (100.0, 101.0, 99.0, 101.0),
        (101.0, 102.0, 100.0, 102.0), // buy here, tp=120
        (102.0, 103.0, 101.0, 103.0), // entry fills at open=102
        (103.0, 104.0, 102.0, 104.0), // tighten tp -> 110 here
        (104.0, 105.0, 103.0, 105.0), // tighten tp -> 108 here
        (105.0, 109.0, 104.0, 108.0), // High touches 108, not 110 or 120
        (108.0, 109.0, 107.0, 108.0),
    ]);
    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 10_000.0,
            commission: Commission::relative(0.0),
            trade_on_close: false,
            ..Default::default()
        },
    );
    let strat = BuyThenTightenTpTwice {
        bought: false,
        tightened_once: false,
        tightened_twice: false,
        trade_id: None,
    };
    let result = bt.run(strat).unwrap();

    assert_eq!(result.closed_trades.len(), 1);
    let t = &result.closed_trades[0];
    assert_eq!(t.exit_bar, Some(6));
    assert!(
        (t.exit_price.unwrap() - 108.0).abs() < 1e-9,
        "should exit at the most recently set TP (108), not an earlier one, got {:?}",
        t.exit_price
    );
}

#[test]
fn custom_commission_charges_the_flat_fee_on_both_legs() {
    struct BuyOnceFlat {
        bought: bool,
    }
    impl Strategy for BuyOnceFlat {
        fn init(&mut self, _ctx: &mut Context) {}
        fn next(&mut self, ctx: &mut Context) {
            if !self.bought && ctx.bar_index() == 1 {
                ctx.buy(OrderSize::Units(10.0), None, None, None, None, None)
                    .unwrap();
                self.bought = true;
            }
        }
    }

    let data = bars(&[
        (100.0, 101.0, 99.0, 100.0),
        (100.0, 101.0, 99.0, 101.0),
        (101.0, 102.0, 100.0, 102.0),
        (102.0, 103.0, 101.0, 103.0),
        (103.0, 104.0, 102.0, 104.0),
    ]);
    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 10_000.0,
            commission: Commission::custom(|_order_size, _price| 5.0),
            trade_on_close: false,
            ..Default::default()
        },
    );
    let result = bt.run(BuyOnceFlat { bought: false }).unwrap();

    assert_eq!(result.closed_trades.len(), 1);
    let t = &result.closed_trades[0];
    assert!(
        (t.commission - 10.0).abs() < 1e-9,
        "flat $5 fee should be charged on both entry and exit legs, got {}",
        t.commission
    );
}

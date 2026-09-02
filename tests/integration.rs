use backtesting::{Backtest, BrokerConfig, Commission, Context, Data, OrderSize, Strategy};
use chrono::NaiveDate;

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
    Data::new(index, open, high, low, close, volume)
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

/// Same setup, but the price dips through the stop instead of rallying
/// through the limit -- the SL (a stop order) should fill, not the TP.
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

/// A trade held open to the end of the data should be force-closed by
/// `finalize_trades` (the default) and show up in `closed_trades`.
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

/// An order sized far beyond available margin should be rejected (with a
/// warning), not silently opened at whatever size fits.
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

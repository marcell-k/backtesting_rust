use backtesting::{
    Backtest, BrokerConfig, Commission, Context, Data, Indicator, OrderSize, Strategy,
};
use chrono::NaiveDate;

const EXPECTED_NUM_TRADES: usize = 534;
const EXPECTED_EQUITY_FINAL: f64 = 25071.721564705353;
const EXPECTED_RETURN_PCT: f64 = 150.71721564705354;
const EXPECTED_WIN_RATE_PCT: f64 = 42.50936329588015;
const EXPECTED_BEST_TRADE_PCT: f64 = 25.57313680818692;
const EXPECTED_WORST_TRADE_PCT: f64 = -20.819888469680105;
const EXPECTED_TRADE_SIZE: i64 = 16;
const EXPECTED_ENTRY_PRICE: f64 = 298.936773;
const EXPECTED_EXIT_PRICE: f64 = 333.137385;
const TOL: f64 = 1e-6;

fn assert_close(label: &str, actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < TOL.max(expected.abs() * 1e-9),
        "{label}: expected {expected}, got {actual}"
    );
}

/// Same warmup convention as `Strategy.I()`: first `window - 1` entries NaN.
fn sma(values: &[f64], window: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    let mut sum = 0.0;
    for i in 0..values.len() {
        sum += values[i];
        if i >= window {
            sum -= values[i - window];
        }
        if i + 1 >= window {
            out[i] = sum / window as f64;
        }
    }
    out
}

/// Identical strategy to `src/bin/sma.rs` and to the Python reference
/// script's `SmaCross`: go long on a fast/slow SMA cross-up, flip short
/// on a cross-down.
struct SmaCross {
    fast_window: usize,
    slow_window: usize,
    sma_fast: usize,
    sma_slow: usize,
}

impl SmaCross {
    fn new(fast_window: usize, slow_window: usize) -> Self {
        Self {
            fast_window,
            slow_window,
            sma_fast: usize::MAX,
            sma_slow: usize::MAX,
        }
    }
}

impl Strategy for SmaCross {
    fn init(&mut self, ctx: &mut Context) {
        let close = ctx.data.full_close();
        self.sma_fast = ctx.indicator(Indicator::new(
            format!("SMA({})", self.fast_window),
            sma(close, self.fast_window),
        ));
        self.sma_slow = ctx.indicator(Indicator::new(
            format!("SMA({})", self.slow_window),
            sma(close, self.slow_window),
        ));
    }

    fn next(&mut self, ctx: &mut Context) {
        let fast = ctx.indicator_series(self.sma_fast);
        let slow = ctx.indicator_series(self.sma_slow);
        if fast.len() < 2 || slow.len() < 2 {
            return;
        }
        let (fast_now, fast_prev) = (fast[fast.len() - 1], fast[fast.len() - 2]);
        let (slow_now, slow_prev) = (slow[slow.len() - 1], slow[slow.len() - 2]);

        let crossed_up = fast_prev <= slow_prev && fast_now > slow_now;
        let crossed_down = fast_prev >= slow_prev && fast_now < slow_now;

        if crossed_up {
            ctx.buy(OrderSize::Fraction(0.001), None, None, None, None, None)
                .unwrap();
        } else if crossed_down {
            ctx.sell(OrderSize::Fraction(0.001), None, None, None, None, None)
                .unwrap();
        }
    }
}

/// Minimal CSV loader for `tests/fixtures/ohlcv.csv`
/// (date,open,high,low,close,volume) -- no external crate needed.
fn load_fixture() -> Data {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ohlcv.csv");
    let content = std::fs::read_to_string(path).expect("read tests/fixtures/ohlcv.csv");

    let mut index = Vec::new();
    let mut open = Vec::new();
    let mut high = Vec::new();
    let mut low = Vec::new();
    let mut close = Vec::new();
    let mut volume = Vec::new();

    for line in content.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(',').collect();
        let date = NaiveDate::parse_from_str(parts[0], "%Y-%m-%d").unwrap();
        index.push(date.and_hms_opt(0, 0, 0).unwrap());
        open.push(parts[1].parse::<f64>().unwrap());
        high.push(parts[2].parse::<f64>().unwrap());
        low.push(parts[3].parse::<f64>().unwrap());
        close.push(parts[4].parse::<f64>().unwrap());
        volume.push(parts[5].parse::<f64>().unwrap());
    }

    Data::new(index, open, high, low, close, volume)
}

#[test]
fn sma_cross_matches_backtesting_py() {
    let data = load_fixture();
    assert_eq!(data.full_len(), 10000, "fixture should have 10000 bars");

    let bt = Backtest::new(
        data,
        BrokerConfig {
            cash: 10_000.0,
            commission: Commission::relative(0.0002),
            margin: 1.0 / 1000.0,
            trade_on_close: true,
            hedging: false,
            exclusive_orders: false,
            ..Default::default()
        },
    );
    let result = bt.run(SmaCross::new(10, 20)).unwrap();

    assert_eq!(
        result.stats.num_trades, EXPECTED_NUM_TRADES,
        "trade count diverged from backtesting.py"
    );
    assert_close(
        "equity_final",
        result.stats.equity_final,
        EXPECTED_EQUITY_FINAL,
    );
    assert_close("return_pct", result.stats.return_pct, EXPECTED_RETURN_PCT);
    assert_close(
        "win_rate_pct",
        result.stats.win_rate_pct,
        EXPECTED_WIN_RATE_PCT,
    );
    assert_close(
        "best_trade_pct",
        result.stats.best_trade_pct,
        EXPECTED_BEST_TRADE_PCT,
    );
    assert_close(
        "worst_trade_pct",
        result.stats.worst_trade_pct,
        EXPECTED_WORST_TRADE_PCT,
    );

    let t = result
        .closed_trades
        .last()
        .expect("expected at least one closed trade");
    assert_eq!(t.size, EXPECTED_TRADE_SIZE, "trade size diverged");
    assert_close("entry_price", t.entry_price, EXPECTED_ENTRY_PRICE);
    assert_close(
        "exit_price",
        t.exit_price.expect("trade should be closed"),
        EXPECTED_EXIT_PRICE,
    );
}

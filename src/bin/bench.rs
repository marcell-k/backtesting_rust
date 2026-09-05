//! Usage:
//!   cargo build --release --bin bench
//!   hyperfine --warmup 3 --min-runs 20 './target/release/bench data.csv'

use backtesting::{
    Backtest, BrokerConfig, Commission, Context, Data, Indicator, OrderSize, Strategy,
};
use chrono::NaiveDate;
use std::env;

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

        let price = ctx.data.full_close()[fast.len() - 1];

        if crossed_up {
            let sl = price * 0.99;
            let tp = price * 1.01;

            ctx.buy(
                OrderSize::Fraction(0.001),
                None,
                None,
                Some(sl),
                Some(tp),
                None,
            )
            .unwrap();
        } else if crossed_down {
            let sl = price * 1.01;
            let tp = price * 0.99;

            ctx.sell(
                OrderSize::Fraction(0.001),
                None,
                None,
                Some(sl),
                Some(tp),
                None,
            )
            .unwrap();
        }
    }
}

fn load_fixture() -> Data {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/data.csv");
    let content = std::fs::read_to_string(path).expect("read data.csv");

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

    Data::new(index, open, high, low, close, volume).unwrap()
}

fn main() {
    let data = load_fixture();
    let n = data.full_len();

    let broker_config = BrokerConfig {
        cash: 1_000_000.0,
        commission: Commission::relative(0.00002),
        margin: 1.0 / 1000.0,
        exclusive_orders: false,
        ..Default::default()
    };

    let bt = Backtest::new(data, broker_config);
    for _ in 0..100 {
        let _ = bt.run(SmaCross::new(10, 20)).expect("backtest run failed");
    }

    let result = bt.run(SmaCross::new(10, 20)).expect("backtest run failed");
    println!(
        "bars={n} trades={} equity_final={:.2} return_pct={:.2}",
        result.stats.num_trades, result.stats.equity_final, result.stats.return_pct
    );
}

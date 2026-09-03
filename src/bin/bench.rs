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

fn parse_date_fast(s: &str) -> NaiveDate {
    let b = s.as_bytes();
    assert!(
        b.len() == 10 && b[4] == b'-' && b[7] == b'-',
        "expected YYYY-MM-DD, got {s:?}"
    );
    let digit = |i: usize| (b[i] - b'0') as i32;
    let year = digit(0) * 1000 + digit(1) * 100 + digit(2) * 10 + digit(3);
    let month = (digit(5) * 10 + digit(6)) as u32;
    let day = (digit(8) * 10 + digit(9)) as u32;
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or_else(|| panic!("bad date {s:?}"))
}

fn load_csv(path: &str) -> Data {
    let content =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));

    // Pre-size the columns instead of letting them grow-and-copy repeatedly.
    let n_rows = content.lines().count().saturating_sub(1);
    let mut index = Vec::with_capacity(n_rows);
    let mut open = Vec::with_capacity(n_rows);
    let mut high = Vec::with_capacity(n_rows);
    let mut low = Vec::with_capacity(n_rows);
    let mut close = Vec::with_capacity(n_rows);
    let mut volume = Vec::with_capacity(n_rows);

    for line in content.lines().skip(1) {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split(',');
        let date_str = fields.next().expect("missing date field");
        let o: f64 = fields.next().expect("missing open").parse().unwrap();
        let h: f64 = fields.next().expect("missing high").parse().unwrap();
        let l: f64 = fields.next().expect("missing low").parse().unwrap();
        let c: f64 = fields.next().expect("missing close").parse().unwrap();
        let v: f64 = fields.next().expect("missing volume").parse().unwrap();

        index.push(parse_date_fast(date_str).and_hms_opt(0, 0, 0).unwrap());
        open.push(o);
        high.push(h);
        low.push(l);
        close.push(c);
        volume.push(v);
    }

    Data::new(index, open, high, low, close, volume)
}
fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| "data.csv".to_string());
    let data = load_csv(&path);
    let n = data.full_len();

    let broker_config = BrokerConfig {
        cash: 10_000.0,
        commission: Commission::relative(0.00002),
        margin: 1.0 / 1000.0,
        exclusive_orders: false,
        ..Default::default()
    };

    let bt = Backtest::new(data, broker_config);
    let result = bt.run(SmaCross::new(10, 20)).expect("backtest run failed");

    println!(
        "bars={n} trades={} equity_final={:.2} return_pct={:.2}",
        result.stats.num_trades, result.stats.equity_final, result.stats.return_pct
    );
}

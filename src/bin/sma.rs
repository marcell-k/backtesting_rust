//! Port of the textbook `backtesting.py` "SMA cross" example:
//! go long when the fast SMA crosses above the slow one, go short (i.e.
//! flip) when it crosses back below.
//!
//! Run with: `cargo run --release --bin sma_cross_demo`

use backtesting::{
    Backtest, BrokerConfig, Commission, Context, Data, Indicator, OrderSize, Stats, Strategy,
};
use chrono::{Duration, NaiveDate};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

/// Simple moving average of `values` over `window` bars; the first
/// `window - 1` entries are `NaN`, matching `Strategy.I()`'s warmup
/// convention in Python.
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
    sma_fast: usize, // indicator handles, filled in by init()
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
            ctx.buy(OrderSize::All, None, None, None, None, None)
                .unwrap();
        } else if crossed_down {
            ctx.sell(OrderSize::All, None, None, None, None, None)
                .unwrap();
        }
    }
}

/// Generates a synthetic daily OHLCV series via geometric random walk, so
/// the demo has no external data dependency.
fn synthetic_data(n_days: usize, seed: u64) -> Data {
    let mut rng = StdRng::seed_from_u64(seed);
    let start_date = NaiveDate::from_ymd_opt(2020, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();

    let mut index = Vec::with_capacity(n_days);
    let mut open = Vec::with_capacity(n_days);
    let mut high = Vec::with_capacity(n_days);
    let mut low = Vec::with_capacity(n_days);
    let mut close = Vec::with_capacity(n_days);
    let mut volume = Vec::with_capacity(n_days);

    let mut price = 100.0_f64;
    for i in 0..n_days {
        index.push(start_date + Duration::days(i as i64));
        let day_open = price;
        let drift = 0.0002;
        let shock: f64 = rng.random_range(-0.02..0.02);
        price = (price * (1.0 + drift + shock)).max(0.5);
        let day_close = price;
        let day_high = day_open.max(day_close) * (1.0 + rng.random_range(0.0..0.01));
        let day_low = day_open.min(day_close) * (1.0 - rng.random_range(0.0..0.01));

        open.push(day_open);
        high.push(day_high);
        low.push(day_low);
        close.push(day_close);
        volume.push(rng.random_range(1_000.0..10_000.0));
    }

    Data::new(index, open, high, low, close, volume)
}

fn print_stats(label: &str, stats: &Stats) {
    println!("== {label} ==");
    println!(
        "  Bars (start..end):      {}..{}",
        stats.start_bar, stats.end_bar
    );
    println!("  Exposure Time [%]:      {:.2}", stats.exposure_time_pct);
    println!("  Equity Final [$]:       {:.2}", stats.equity_final);
    println!("  Equity Peak [$]:        {:.2}", stats.equity_peak);
    println!("  Return [%]:             {:.2}", stats.return_pct);
    println!(
        "  Buy & Hold Return [%]:  {:.2}",
        stats.buy_and_hold_return_pct
    );
    println!("  Return (Ann.) [%]:      {:.2}", stats.return_ann_pct);
    println!("  Volatility (Ann.) [%]:  {:.2}", stats.volatility_ann_pct);
    println!("  Sharpe Ratio:           {:.2}", stats.sharpe_ratio);
    println!("  Sortino Ratio:          {:.2}", stats.sortino_ratio);
    println!("  Calmar Ratio:           {:.2}", stats.calmar_ratio);
    println!("  Max. Drawdown [%]:      {:.2}", stats.max_drawdown_pct);
    println!("  Avg. Drawdown [%]:      {:.2}", stats.avg_drawdown_pct);
    println!("  # Trades:               {}", stats.num_trades);
    println!("  Win Rate [%]:           {:.2}", stats.win_rate_pct);
    println!("  Best Trade [%]:         {:.2}", stats.best_trade_pct);
    println!("  Worst Trade [%]:        {:.2}", stats.worst_trade_pct);
    println!("  Avg. Trade [%]:         {:.2}", stats.avg_trade_pct);
    println!("  Profit Factor:          {:.2}", stats.profit_factor);
    println!("  Expectancy [%]:         {:.2}", stats.expectancy_pct);
    println!("  SQN:                    {:.2}", stats.sqn);
}

fn main() {
    let data = synthetic_data(1000, 42);

    let broker_config = BrokerConfig {
        cash: 10_000.0,
        commission: Commission::relative(0.002),
        ..Default::default()
    };

    let bt = Backtest::new(data, broker_config);

    // A single run, like `bt.run()`.
    let result = bt.run(SmaCross::new(10, 20)).expect("backtest run failed");
    print_stats("SmaCross(10, 20)", &result.stats);
    if !result.warnings.is_empty() {
        println!(
            "  ({} warning(s) emitted during the run)",
            result.warnings.len()
        );
    }

    // A small parallel grid search, like `bt.optimize(fast=..., slow=...)`.
    let mut candidates = Vec::new();
    for fast in [5, 10, 15, 20] {
        for slow in [20, 30, 40, 50] {
            if fast < slow {
                candidates.push(SmaCross::new(fast, slow));
            }
        }
    }
    let (best_idx, best) = bt.optimize(candidates, |s| s.sqn).expect("optimize failed");
    println!();
    println!(
        "Best of grid search: SmaCross({}, {}) [candidate #{best_idx}]",
        best.strategy.fast_window, best.strategy.slow_window
    );
    print_stats("Best candidate", &best.stats);
}

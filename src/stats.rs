use crate::trade::Trade;

#[derive(Debug, Clone)]
pub struct Stats {
    pub start_bar: usize,
    pub end_bar: usize,
    pub duration_bars: usize,
    pub exposure_time_pct: f64,

    pub equity_final: f64,
    pub equity_peak: f64,
    pub return_pct: f64,
    pub buy_and_hold_return_pct: f64,
    pub return_ann_pct: f64,
    pub volatility_ann_pct: f64,

    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub calmar_ratio: f64,

    pub max_drawdown_pct: f64,
    pub avg_drawdown_pct: f64,
    pub max_drawdown_duration_bars: usize,
    pub avg_drawdown_duration_bars: f64,

    pub num_trades: usize,
    pub win_rate_pct: f64,
    pub best_trade_pct: f64,
    pub worst_trade_pct: f64,
    pub avg_trade_pct: f64,
    pub max_trade_duration_bars: usize,
    pub avg_trade_duration_bars: f64,
    pub profit_factor: f64,
    pub expectancy_pct: f64,

    pub sqn: f64,
}
use std::fmt;

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "  Bars (start..end):      {}..{}",
            self.start_bar, self.end_bar
        )?;
        writeln!(f, "  Exposure Time [%]:      {:.2}", self.exposure_time_pct)?;
        writeln!(f, "  Equity Final [$]:       {:.2}", self.equity_final)?;
        writeln!(f, "  Equity Peak [$]:        {:.2}", self.equity_peak)?;
        writeln!(f, "  Return [%]:             {:.2}", self.return_pct)?;
        writeln!(
            f,
            "  Buy & Hold Return [%]:  {:.2}",
            self.buy_and_hold_return_pct
        )?;
        writeln!(f, "  Return (Ann.) [%]:      {:.2}", self.return_ann_pct)?;
        writeln!(
            f,
            "  Volatility (Ann.) [%]:  {:.2}",
            self.volatility_ann_pct
        )?;
        writeln!(f, "  Sharpe Ratio:           {:.2}", self.sharpe_ratio)?;
        writeln!(f, "  Sortino Ratio:          {:.2}", self.sortino_ratio)?;
        writeln!(f, "  Calmar Ratio:           {:.2}", self.calmar_ratio)?;
        writeln!(f, "  Max. Drawdown [%]:      {:.2}", self.max_drawdown_pct)?;
        writeln!(f, "  Avg. Drawdown [%]:      {:.2}", self.avg_drawdown_pct)?;
        writeln!(f, "  # Trades:               {}", self.num_trades)?;
        writeln!(f, "  Win Rate [%]:           {:.2}", self.win_rate_pct)?;
        writeln!(f, "  Best Trade [%]:         {:.2}", self.best_trade_pct)?;
        writeln!(f, "  Worst Trade [%]:        {:.2}", self.worst_trade_pct)?;
        writeln!(f, "  Avg. Trade [%]:         {:.2}", self.avg_trade_pct)?;
        writeln!(f, "  Profit Factor:          {:.2}", self.profit_factor)?;
        writeln!(f, "  Expectancy [%]:         {:.2}", self.expectancy_pct)?;
        write!(f, "  SQN:                    {:.2}", self.sqn)
    }
}

pub fn print_stats(label: &str, stats: &Stats) {
    println!("== {label} ==");
    println!("{stats}");
}
pub fn compute_stats(
    equity_curve: &[f64],
    closed_trades: &[Trade],
    close: &[f64],
    start_bar: usize,
    periods_per_year: f64,
) -> Stats {
    let n_full = equity_curve.len();
    let end_bar = n_full.saturating_sub(1);
    let equity_final = *equity_curve.last().unwrap_or(&0.0);

    let equity_peak = equity_curve
        .iter()
        .cloned()
        .fold(f64::MIN, f64::max)
        .max(equity_final);

    // -- per-bar returns & drawdown --
    let mut returns = Vec::with_capacity(n_full.saturating_sub(1));
    for w in equity_curve.windows(2) {
        if w[0] != 0.0 {
            returns.push(w[1] / w[0] - 1.0);
        }
    }

    let mut peak = f64::MIN;
    let mut drawdowns = Vec::with_capacity(n_full);
    let mut dd_durations = Vec::new();
    let mut cur_dd_len = 0usize;
    for &eq in equity_curve {
        peak = peak.max(eq);
        let dd = if peak > 0.0 { (peak - eq) / peak } else { 0.0 };
        drawdowns.push(dd);
        if dd > 0.0 {
            cur_dd_len += 1;
        } else if cur_dd_len > 0 {
            dd_durations.push(cur_dd_len);
            cur_dd_len = 0;
        }
    }
    if cur_dd_len > 0 {
        dd_durations.push(cur_dd_len);
    }
    let max_drawdown_pct = drawdowns.iter().cloned().fold(0.0_f64, f64::max) * 100.0;
    let avg_drawdown_pct = if drawdowns.iter().any(|&d| d > 0.0) {
        let positive: Vec<f64> = drawdowns.iter().cloned().filter(|&d| d > 0.0).collect();
        mean(&positive) * 100.0
    } else {
        0.0
    };
    let max_drawdown_duration_bars = dd_durations.iter().copied().max().unwrap_or(0);
    let avg_drawdown_duration_bars = if dd_durations.is_empty() {
        0.0
    } else {
        dd_durations.iter().sum::<usize>() as f64 / dd_durations.len() as f64
    };

    let cash0 = equity_curve.first().copied().unwrap_or(equity_final);
    let return_pct = if cash0 != 0.0 {
        (equity_final / cash0 - 1.0) * 100.0
    } else {
        0.0
    };

    let bh_start = close.first().copied().unwrap_or(1.0);
    let bh_end = close.last().copied().unwrap_or(bh_start);
    let buy_and_hold_return_pct = if bh_start != 0.0 {
        (bh_end / bh_start - 1.0) * 100.0
    } else {
        0.0
    };

    let years = if periods_per_year > 0.0 {
        n_full.saturating_sub(start_bar) as f64 / periods_per_year
    } else {
        0.0
    };
    let return_ann_pct = if years > 0.0 && cash0 != 0.0 {
        (((equity_final / cash0).powf(1.0 / years)) - 1.0) * 100.0
    } else {
        0.0
    };
    let volatility_ann_pct = if periods_per_year > 0.0 {
        std_dev(&returns) * periods_per_year.sqrt() * 100.0
    } else {
        0.0
    };

    let sharpe_ratio = {
        let m = mean(&returns);
        let s = std_dev(&returns);
        if s > 0.0 && periods_per_year > 0.0 {
            m / s * periods_per_year.sqrt()
        } else {
            0.0
        }
    };
    let sortino_ratio = {
        let m = mean(&returns);
        let downside: Vec<f64> = returns.iter().cloned().filter(|&r| r < 0.0).collect();
        let ds = std_dev(&downside);
        if ds > 0.0 && periods_per_year > 0.0 {
            m / ds * periods_per_year.sqrt()
        } else {
            0.0
        }
    };
    let calmar_ratio = if max_drawdown_pct > 0.0 {
        return_ann_pct / max_drawdown_pct
    } else {
        0.0
    };

    // -- trade-level stats --
    let num_trades = closed_trades.len();
    let last_price = close.last().copied().unwrap_or(0.0);
    let trade_pl_pcts: Vec<f64> = closed_trades
        .iter()
        .map(|t| t.pl_pct(last_price) * 100.0)
        .collect();
    let trade_pls: Vec<f64> = closed_trades.iter().map(|t| t.pl(last_price)).collect();
    let trade_durations: Vec<usize> = closed_trades
        .iter()
        .map(|t| {
            t.exit_bar
                .unwrap_or(t.entry_bar)
                .saturating_sub(t.entry_bar)
        })
        .collect();

    let win_rate_pct = if num_trades > 0 {
        trade_pls.iter().filter(|&&pl| pl > 0.0).count() as f64 / num_trades as f64 * 100.0
    } else {
        0.0
    };
    let best_trade_pct = trade_pl_pcts.iter().cloned().fold(f64::MIN, f64::max);
    let worst_trade_pct = trade_pl_pcts.iter().cloned().fold(f64::MAX, f64::min);
    let avg_trade_pct = mean(&trade_pl_pcts);
    let max_trade_duration_bars = trade_durations.iter().copied().max().unwrap_or(0);
    let avg_trade_duration_bars = if trade_durations.is_empty() {
        0.0
    } else {
        trade_durations.iter().sum::<usize>() as f64 / trade_durations.len() as f64
    };

    let gross_profit: f64 = trade_pls.iter().cloned().filter(|&pl| pl > 0.0).sum();
    let gross_loss: f64 = trade_pls
        .iter()
        .cloned()
        .filter(|&pl| pl < 0.0)
        .sum::<f64>()
        .abs();
    let profit_factor = if gross_loss > 0.0 {
        gross_profit / gross_loss
    } else {
        f64::INFINITY
    };

    let expectancy_pct = avg_trade_pct;

    let sqn = {
        let m = mean(&trade_pls);
        let s = std_dev(&trade_pls);
        if s > 0.0 && num_trades > 0 {
            m / s * (num_trades as f64).sqrt()
        } else {
            0.0
        }
    };

    let window_len = end_bar.saturating_sub(start_bar) + 1;
    let exposure_time_pct = if window_len > 0 {
        // Fraction of bars (within the post-warmup tradable window) during
        // which at least one trade was open. Approximated from trade
        // entry/exit bar indices.
        let mut open_bars = vec![false; window_len];
        for t in closed_trades {
            let entry = t.entry_bar.saturating_sub(start_bar).min(window_len - 1);
            let exit = t
                .exit_bar
                .unwrap_or(end_bar)
                .saturating_sub(start_bar)
                .min(window_len - 1);
            for b in open_bars.iter_mut().take(exit + 1).skip(entry) {
                *b = true;
            }
        }
        open_bars.iter().filter(|&&b| b).count() as f64 / window_len as f64 * 100.0
    } else {
        0.0
    };

    Stats {
        start_bar,
        end_bar,
        duration_bars: end_bar.saturating_sub(start_bar),
        exposure_time_pct,
        equity_final,
        equity_peak,
        return_pct,
        buy_and_hold_return_pct,
        return_ann_pct,
        volatility_ann_pct,
        sharpe_ratio,
        sortino_ratio,
        calmar_ratio,
        max_drawdown_pct,
        avg_drawdown_pct,
        max_drawdown_duration_bars,
        avg_drawdown_duration_bars,
        num_trades,
        win_rate_pct,
        best_trade_pct: if best_trade_pct.is_finite() {
            best_trade_pct
        } else {
            0.0
        },
        worst_trade_pct: if worst_trade_pct.is_finite() {
            worst_trade_pct
        } else {
            0.0
        },
        avg_trade_pct,
        max_trade_duration_bars,
        avg_trade_duration_bars,
        profit_factor,
        expectancy_pct,
        sqn,
    }
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

fn std_dev(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let m = mean(xs);
    let var = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() as f64 - 1.0);
    var.sqrt()
}

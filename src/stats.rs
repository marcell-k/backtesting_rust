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

#[derive(Default, Clone, Copy)]
struct RunningStats {
    sum: f64,
    sumsq: f64,
    n: usize,
}

impl RunningStats {
    #[inline]
    fn push(&mut self, x: f64) {
        self.sum += x;
        self.sumsq += x * x;
        self.n += 1;
    }

    fn mean_std(&self) -> (f64, f64) {
        if self.n == 0 {
            return (0.0, 0.0);
        }
        let mean = self.sum / self.n as f64;
        if self.n < 2 {
            return (mean, 0.0);
        }
        let var = (self.sumsq - self.sum * self.sum / self.n as f64) / (self.n as f64 - 1.0);
        (mean, var.max(0.0).sqrt())
    }
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

    let mut peak = f64::MIN;
    let mut max_drawdown = 0.0_f64;
    let mut dd_sum = 0.0_f64;
    let mut dd_count = 0usize;
    let mut dd_durations = Vec::new();
    let mut cur_dd_len = 0usize;

    let mut returns_stats = RunningStats::default();
    let mut downside_stats = RunningStats::default();

    let mut prev_eq: Option<f64> = None;

    for &eq in equity_curve {
        // drawdown tracking
        peak = peak.max(eq);
        let dd = if peak > 0.0 { (peak - eq) / peak } else { 0.0 };
        if dd > max_drawdown {
            max_drawdown = dd;
        }
        if dd > 0.0 {
            dd_sum += dd;
            dd_count += 1;
            cur_dd_len += 1;
        } else if cur_dd_len > 0 {
            dd_durations.push(cur_dd_len);
            cur_dd_len = 0;
        }

        // per-bar return tracking
        if let Some(p) = prev_eq
            && p != 0.0
        {
            let r = eq / p - 1.0;
            returns_stats.push(r);
            if r < 0.0 {
                downside_stats.push(r);
            }
        }
        prev_eq = Some(eq);
    }
    if cur_dd_len > 0 {
        dd_durations.push(cur_dd_len);
    }

    let equity_peak = peak.max(equity_final);
    let max_drawdown_pct = max_drawdown * 100.0;
    let avg_drawdown_pct = if dd_count > 0 {
        dd_sum / dd_count as f64 * 100.0
    } else {
        0.0
    };
    let max_drawdown_duration_bars = dd_durations.iter().copied().max().unwrap_or(0);
    let avg_drawdown_duration_bars = if dd_durations.is_empty() {
        0.0
    } else {
        dd_durations.iter().sum::<usize>() as f64 / dd_durations.len() as f64
    };

    let (mean_r, std_r) = returns_stats.mean_std();
    let (_, std_downside) = downside_stats.mean_std();

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
        std_r * periods_per_year.sqrt() * 100.0
    } else {
        0.0
    };

    let sharpe_ratio = if std_r > 0.0 && periods_per_year > 0.0 {
        mean_r / std_r * periods_per_year.sqrt()
    } else {
        0.0
    };
    let sortino_ratio = if std_downside > 0.0 && periods_per_year > 0.0 {
        mean_r / std_downside * periods_per_year.sqrt()
    } else {
        0.0
    };
    let calmar_ratio = if max_drawdown_pct > 0.0 {
        return_ann_pct / max_drawdown_pct
    } else {
        0.0
    };

    // -- trade-level stats --
    let num_trades = closed_trades.len();
    let last_price = close.last().copied().unwrap_or(0.0);
    let mut win_count = 0usize;
    let mut sum_pct = 0.0_f64;
    let mut best_trade_pct = f64::MIN;
    let mut worst_trade_pct = f64::MAX;
    let mut max_trade_duration_bars = 0usize;
    let mut sum_duration = 0usize;
    let mut gross_profit = 0.0_f64;
    let mut gross_loss = 0.0_f64;
    let mut pl_stats = RunningStats::default();

    for t in closed_trades {
        let pl = t.pl(last_price);
        let pct = t.pl_pct(last_price) * 100.0;
        let duration = t
            .exit_bar
            .unwrap_or(t.entry_bar)
            .saturating_sub(t.entry_bar);

        if pl > 0.0 {
            win_count += 1;
            gross_profit += pl;
        } else if pl < 0.0 {
            gross_loss -= pl;
        }

        sum_pct += pct;
        best_trade_pct = best_trade_pct.max(pct);
        worst_trade_pct = worst_trade_pct.min(pct);

        max_trade_duration_bars = max_trade_duration_bars.max(duration);
        sum_duration += duration;

        pl_stats.push(pl);
    }

    let win_rate_pct = if num_trades > 0 {
        win_count as f64 / num_trades as f64 * 100.0
    } else {
        0.0
    };
    let avg_trade_pct = if num_trades > 0 {
        sum_pct / num_trades as f64
    } else {
        0.0
    };
    let avg_trade_duration_bars = if num_trades > 0 {
        sum_duration as f64 / num_trades as f64
    } else {
        0.0
    };
    let profit_factor = if gross_loss > 0.0 {
        gross_profit / gross_loss
    } else {
        f64::INFINITY
    };
    let expectancy_pct = avg_trade_pct;
    let (mean_pl, std_pl) = pl_stats.mean_std();
    let sqn = if std_pl > 0.0 && num_trades > 0 {
        mean_pl / std_pl * (num_trades as f64).sqrt()
    } else {
        0.0
    };

    let window_len = end_bar.saturating_sub(start_bar) + 1;
    let exposure_time_pct = if window_len > 0 {
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

use crate::{
    broker::{Broker, BrokerConfig},
    data::Data,
    error::{BacktestError, BtResult},
    indicator::{Indicator, warmup_start_bar},
    stats::{Stats, compute_stats},
    strategy::{Context, Strategy},
    trade::Trade,
};

pub struct RunResult<S> {
    pub strategy: S,
    pub stats: Stats,
    pub closed_trades: Vec<Trade>,
    pub equity_curve: Vec<f64>,
    pub warnings: Vec<String>,
}

pub struct Backtest {
    data: Data,
    broker_config: BrokerConfig,
    finalize_trades: bool,
    periods_per_year: f64,
}

impl Backtest {
    pub fn new(data: Data, broker_config: BrokerConfig) -> Self {
        Self {
            data,
            broker_config,
            finalize_trades: true,
            periods_per_year: 252.0,
        }
    }

    pub fn finalize_trades(mut self, v: bool) -> Self {
        self.finalize_trades = v;
        self
    }

    pub fn periods_per_year(mut self, v: f64) -> Self {
        self.periods_per_year = v;
        self
    }

    pub fn run<S: Strategy>(&self, mut strategy: S) -> BtResult<RunResult<S>> {
        let mut data = self.data.clone();
        let full_len = data.full_len();
        let mut broker = Broker::new(self.broker_config.clone(), full_len)?;
        let mut indicators: Vec<Indicator> = Vec::new();

        data.set_length(full_len);
        {
            let mut ctx = Context {
                data: &data,
                broker: &mut broker,
                indicators: &mut indicators,
                bar_index: full_len.saturating_sub(1),
            };
            strategy.init(&mut ctx);
        }
        // skip warmup bars
        let start = warmup_start_bar(&indicators).min(full_len);

        let mut ran_out_of_money = false;
        let mut last_bar = start.saturating_sub(1);

        for i in start..full_len {
            data.set_length(i + 1);

            match broker.advance(&data, i) {
                Ok(()) => {}
                Err(BacktestError::OutOfMoney) => {
                    ran_out_of_money = true;
                    last_bar = i;
                    break;
                }
                Err(other) => return Err(other),
            }

            last_bar = i;

            let mut ctx = Context {
                data: &data,
                broker: &mut broker,
                indicators: &mut indicators,
                bar_index: i,
            };
            strategy.next(&mut ctx);
        }

        if !ran_out_of_money && self.finalize_trades {
            let ids: Vec<_> = broker.trades().iter().rev().map(|t| t.id).collect();
            for tid in ids {
                broker.request_trade_close(tid, 1.0)?;
            }
            // re-run broker once more on the same last bar's OHLC so the just-placed closing orders
            // fill updating that bar's equity in place
            if start < full_len {
                let _ = broker.advance(&data, last_bar);
            }
        }

        data.set_length(full_len);

        let cash = broker.cash();
        let equity_curve = backfill(broker.take_equity_curve(), cash);
        let closed_trades = broker.take_closed_trades();
        let stats = compute_stats(
            &equity_curve,
            &closed_trades,
            data.full_close(),
            start,
            self.periods_per_year,
        );

        Ok(RunResult {
            strategy,
            stats,
            closed_trades,
            equity_curve,
            warnings: std::mem::take(&mut broker.warnings),
        })
    }

    pub fn optimize<S, M>(&self, candidates: Vec<S>, maximize: M) -> BtResult<(usize, RunResult<S>)>
    where
        S: Strategy + Send,
        M: Fn(&Stats) -> f64 + Sync,
    {
        use rayon::prelude::*;

        if candidates.is_empty() {
            return Err(BacktestError::InvalidParameter(
                "need at least one candidate to optimize over".into(),
            ));
        }

        let mut runs: Vec<(usize, RunResult<S>, f64)> = candidates
            .into_par_iter()
            .enumerate()
            .map(|(i, s)| self.run(s).map(|r| (i, r, 0.0)))
            .collect::<BtResult<Vec<_>>>()?;

        for (_, run, score) in runs.iter_mut() {
            *score = maximize(&run.stats);
        }
        runs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        let (idx, result, _) = runs.into_iter().next().expect("checked non-empty above");
        Ok((idx, result))
    }
}

pub fn backfill(mut equity: Vec<f64>, cash: f64) -> Vec<f64> {
    let mut next_valid: Option<f64> = None;
    for v in equity.iter_mut().rev() {
        if v.is_nan() {
            *v = next_valid.unwrap_or(cash);
        } else {
            next_valid = Some(*v);
        }
    }
    equity
}

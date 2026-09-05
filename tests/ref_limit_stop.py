import json
import sys
import warnings

import pandas as pd
from backtesting import Backtest, Strategy
from backtesting.lib import crossover

warnings.filterwarnings(
    "ignore",
    message=r".*A contingent SL/TP order would execute in the same bar.*",
    category=UserWarning,
)
df = pd.read_csv("../data.csv", parse_dates=["Date"]).set_index("Date")
df = df.rename(
    columns={
        "open": "Open",
        "high": "High",
        "low": "Low",
        "close": "Close",
        "volume": "Volume",
    }
)


class SmaCrossLimitStop(Strategy):
    fast_window = 10
    slow_window = 20

    def init(self):
        close = pd.Series(self.data.Close)
        self.sma_fast = self.I(lambda: close.rolling(self.fast_window).mean())
        self.sma_slow = self.I(lambda: close.rolling(self.slow_window).mean())

    def next(self):
        price = self.data.Close[-1]
        if crossover(self.sma_fast, self.sma_slow):
            # breakout entry: buy STOP above current price
            self.buy(
                size=0.0001,
                stop=price * 1.001,
                sl=price * 0.99,
                tp=price * 1.02,
                tag="long_stop_entry",
            )
        elif crossover(self.sma_slow, self.sma_fast):
            # pullback entry: sell LIMIT above current price
            self.sell(
                size=0.0001,
                limit=price * 1.001,
                sl=price * 1.01,
                tp=price * 0.98,
                tag="short_limit_entry",
            )


bt = Backtest(
    df,
    SmaCrossLimitStop,
    cash=1_000_000.0,
    commission=0.0002,
    margin=1.0 / 1000.0,
    trade_on_close=True,
    hedging=False,
    exclusive_orders=False,
    finalize_trades=True,
)
stats = bt.run()
trades = stats["_trades"]

last_trade = None
if not trades.empty:
    t = trades.iloc[-1]
    last_trade = {
        "size": int(t.Size),
        "entry_price": float(t.EntryPrice),
        "exit_price": float(t.ExitPrice) if pd.notna(t.ExitPrice) else None,
        "tag": str(t.Tag) if "Tag" in trades.columns else None,
    }

out = {
    "num_trades": int(stats["# Trades"]),
    "equity_final": float(stats["Equity Final [$]"]),
    "return_pct": float(stats["Return [%]"]),
    "win_rate_pct": float(stats["Win Rate [%]"]),
    "best_trade_pct": float(stats["Best Trade [%]"]),
    "worst_trade_pct": float(stats["Worst Trade [%]"]),
    "last_trade": last_trade,
}

if "--dump-trades" in sys.argv:
    out_df = trades[["Size", "EntryBar", "ExitBar", "EntryPrice", "ExitPrice", "Tag"]].copy()
    out_df.columns = ["size", "entry_bar", "exit_bar", "entry_price", "exit_price", "tag"]
    out_df.to_csv("trades_py.csv", index=False)
    print(f"wrote {len(out_df)} trades to trades_py.csv", file=sys.stderr)
    sys.exit(0)
print(json.dumps(out, indent=2))

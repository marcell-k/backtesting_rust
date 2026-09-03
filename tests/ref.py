import json

import pandas as pd
from backtesting import Backtest, Strategy
from backtesting.lib import crossover

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


class SmaCross(Strategy):
    fast_window = 10
    slow_window = 20

    def init(self):
        close = pd.Series(self.data.Close)
        self.sma_fast = self.I(lambda: close.rolling(self.fast_window).mean())
        self.sma_slow = self.I(lambda: close.rolling(self.slow_window).mean())

    def next(self):
        price = self.data.Close[-1]
        if crossover(self.sma_fast, self.sma_slow):
            self.buy(
                size=0.0001,
                sl=price * 0.99,  # 1% Stop Loss below entry
                tp=price * 1.01,  # 1% Take Profit above entry
            )
        elif crossover(self.sma_slow, self.sma_fast):
            self.sell(
                size=0.0001,
                sl=price * 1.01,  # 1% Stop Loss above entry
                tp=price * 0.99,  # 1% Take Profit below entry
            )


bt = Backtest(
    df,
    SmaCross,
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
        "exit_price": float(t.ExitPrice),
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

print(json.dumps(out, indent=2))

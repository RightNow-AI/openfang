---
name: trader-hand-skill
version: "1.0.0"
description: "Expert knowledge for autonomous market intelligence and trading — technical analysis, risk management, Alpaca API, financial data sources"
author: OpenFang
tags: [trading, finance, stocks, crypto, technical-analysis, risk-management]
tools: [shell_exec, file_read, file_write, web_fetch, web_search, memory_store]
runtime: prompt_only
---

# Trading Expert Knowledge

## Reference Knowledge

## 1. Technical Analysis Indicators Reference

### RSI (Relative Strength Index)
```
Formula: RSI = 100 - (100 / (1 + RS))
Where:  RS = Average Gain / Average Loss over N periods (default N = 14)

Step-by-step calculation:
  1. For each period, compute change = Close(t) - Close(t-1)
  2. Gains = max(change, 0), Losses = abs(min(change, 0))
  3. First average: simple mean of first 14 gains/losses
  4. Subsequent: AvgGain = (PrevAvgGain * 13 + CurrentGain) / 14  (Wilder smoothing)
  5. RS = AvgGain / AvgLoss
  6. RSI = 100 - (100 / (1 + RS))

Worked example (14-period):
  Avg Gain over 14 periods = 1.02
  Avg Loss over 14 periods = 0.68
  RS = 1.02 / 0.68 = 1.50
  RSI = 100 - (100 / (1 + 1.50)) = 100 - 40 = 60.0
```

**Interpretation:**
- RSI < 30: Oversold territory (potential buy signal)
- RSI > 70: Overbought territory (potential sell signal)
- RSI = 50: Neutral — price momentum balanced

**Advanced RSI Signals:**
| Signal | Description | Strength |
|--------|-------------|----------|
| Bearish divergence | Price makes new high, RSI makes lower high | Strong reversal warning |
| Bullish divergence | Price makes new low, RSI makes higher low | Strong reversal warning |
| Bullish failure swing | RSI drops below 30, bounces, pulls back above 30, breaks prior RSI high | Very strong buy |
| Bearish failure swing | RSI rises above 70, drops, bounces below 70, breaks prior RSI low | Very strong sell |
| Range shift | RSI oscillates 40-80 in uptrend, 20-60 in downtrend | Trend confirmation |

**Best practices:** Never use RSI as a sole signal. Combine with trend direction (moving averages) and volume. In strong trends, RSI can stay overbought/oversold for extended periods.

---

### MACD (Moving Average Convergence Divergence)
```
MACD Line   = EMA(12) - EMA(26)
Signal Line = EMA(9) of MACD Line
Histogram   = MACD Line - Signal Line

EMA formula: EMA(t) = Price(t) * k + EMA(t-1) * (1 - k)
Where: k = 2 / (N + 1)
  For EMA(12): k = 2/13 = 0.1538
  For EMA(26): k = 2/27 = 0.0741

Worked example:
  EMA(12) = 155.20
  EMA(26) = 152.80
  MACD Line = 155.20 - 152.80 = 2.40
  Previous Signal Line = 1.80
  Signal Line = 2.40 * (2/10) + 1.80 * (8/10) = 0.48 + 1.44 = 1.92
  Histogram = 2.40 - 1.92 = 0.48 (positive = bullish momentum increasing)
```

**Interpretation:**
| Signal | Condition | Strength |
|--------|-----------|----------|
| Bullish crossover | MACD crosses above Signal Line | Moderate buy |
| Bearish crossover | MACD crosses below Signal Line | Moderate sell |
| Zero-line bullish cross | MACD crosses above zero | Trend change to bullish |
| Zero-line bearish cross | MACD crosses below zero | Trend change to bearish |
| Histogram expansion | Bars growing taller | Momentum accelerating |
| Histogram contraction | Bars shrinking | Momentum weakening, reversal may come |
| Bullish divergence | Price new low, MACD higher low | Strong reversal signal |
| Bearish divergence | Price new high, MACD lower high | Strong reversal signal |

---

### Bollinger Bands
```
Middle Band = SMA(20)
Upper Band  = SMA(20) + 2 * StdDev(20)
Lower Band  = SMA(20) - 2 * StdDev(20)
Bandwidth   = (Upper - Lower) / Middle
%B          = (Price - Lower) / (Upper - Lower)

Worked example:
  SMA(20) = 150.00
  StdDev(20) = 3.50
  Upper = 150.00 + 2 * 3.50 = 157.00
  Lower = 150.00 - 2 * 3.50 = 143.00
  Bandwidth = (157.00 - 143.00) / 150.00 = 0.0933 (9.33%)
  Current price = 155.00
  %B = (155.00 - 143.00) / (157.00 - 143.00) = 12/14 = 0.857
  Interpretation: Price is 85.7% of the way from lower to upper band — near upper band
```

**Key Bollinger Band Signals:**
| Signal | Condition | Meaning |
|--------|-----------|---------|
| Squeeze | Bandwidth at 6-month low | Volatility contraction, big move imminent |
| Squeeze breakout up | Price breaks above upper band after squeeze | Strong bullish breakout |
| Squeeze breakout down | Price breaks below lower band after squeeze | Strong bearish breakout |
| Walking the upper band | Price hugs upper band with middle band rising | Strong uptrend — do NOT short |
| Walking the lower band | Price hugs lower band with middle band falling | Strong downtrend — do NOT buy |
| Mean reversion touch | Price touches outer band, %B reverses | Potential reversion to middle band |
| W-bottom | Price hits lower band twice, second low has higher %B | Bullish reversal pattern |
| M-top | Price hits upper band twice, second high has lower %B | Bearish reversal pattern |

---

### VWAP (Volume Weighted Average Price)
```
VWAP = Cumulative(Typical Price * Volume) / Cumulative(Volume)
Typical Price = (High + Low + Close) / 3

Worked example (first 3 bars of the day):
  Bar 1: TP = (101+99+100)/3 = 100.00, Vol = 10,000 -> cumTP*V = 1,000,000
  Bar 2: TP = (102+100+101)/3 = 101.00, Vol = 15,000 -> cumTP*V = 2,515,000
  Bar 3: TP = (103+101+102)/3 = 102.00, Vol = 8,000  -> cumTP*V = 3,331,000
  Cumulative Volume = 33,000
  VWAP = 3,331,000 / 33,000 = 100.94
```

**Usage:**
- **Institutional benchmark**: If price > VWAP, buyers dominate; price < VWAP, sellers dominate
- **Intraday S/R**: VWAP acts as dynamic support in uptrends, resistance in downtrends
- **Entry filter**: Buy only when price pulls back to VWAP (not chasing extended moves)
- **Standard deviations**: VWAP +1/-1 and +2/-2 StdDev bands serve as profit targets
- **Resets daily**: Do NOT carry VWAP across sessions — it is an intraday metric

---

### Moving Averages
```
SMA(N) = (Close_1 + Close_2 + ... + Close_N) / N
EMA(N) = Close * (2/(N+1)) + PrevEMA * (1 - 2/(N+1))

Key Moving Averages:
  EMA(9)   — very short-term trend (scalping, day trading)
  EMA(20)  — short-term trend
  EMA(50)  — medium-term trend
  SMA(100) — intermediate trend
  SMA(200) — long-term trend (institutional benchmark)
```

**Critical Cross Signals:**
| Cross | Name | Meaning | Reliability |
|-------|------|---------|-------------|
| 50 MA > 200 MA | Golden Cross | Bullish trend reversal | High (lag ~2 weeks) |
| 50 MA < 200 MA | Death Cross | Bearish trend reversal | High (lag ~2 weeks) |
| 9 EMA > 21 EMA | Fast bullish cross | Short-term momentum shift | Moderate |
| Price > 200 SMA | Above long-term trend | Bullish regime | Very High |
| Price < 200 SMA | Below long-term trend | Bearish regime | Very High |

**Moving Average Ribbon** (20/50/100/200 MAs all fanning out): Indicates a very strong trend. When all are stacked in order (20 > 50 > 100 > 200 for uptrend), the trend is highly reliable.

---

### ATR (Average True Range)
```
True Range = max(High - Low, |High - PrevClose|, |Low - PrevClose|)
ATR(14) = Simple or Wilder Moving Average of True Range over 14 periods

Worked example:
  Today: High = 105, Low = 101, PrevClose = 102
  TR = max(105-101, |105-102|, |101-102|) = max(4, 3, 1) = 4
  If ATR(14) was 3.50 yesterday:
  ATR(14) = (3.50 * 13 + 4) / 14 = (45.50 + 4) / 14 = 3.536
```

**Practical Applications:**
| Use Case | Formula | Example |
|----------|---------|---------|
| Stop-loss placement | Entry - 2 * ATR | Entry $100, ATR $2.50 -> Stop at $95.00 |
| Take-profit target | Entry + 3 * ATR | Entry $100, ATR $2.50 -> Target $107.50 |
| Position sizing | Risk$ / ATR | $200 risk / $2.50 ATR = 80 shares |
| Volatility filter | ATR > threshold | Only trade when ATR > daily average (avoid dead markets) |
| Trailing stop | Highest close - 3 * ATR | Locks in profit as price rises |

---

### Volume Analysis
```
OBV (On-Balance Volume):
  If Close > PrevClose: OBV = PrevOBV + Volume
  If Close < PrevClose: OBV = PrevOBV - Volume
  If Close = PrevClose: OBV = PrevOBV

Volume Rate of Change: VROC = (Volume - Volume_N_ago) / Volume_N_ago * 100
```

**Volume Confirmation Rules:**
| Price Action | Volume | Interpretation |
|-------------|--------|----------------|
| Price up | Volume up | Strong bullish — legitimate move |
| Price up | Volume down | Weak rally — likely to reverse |
| Price down | Volume up | Strong bearish — capitulation or breakdown |
| Price down | Volume down | Weak decline — may be nearing bottom |
| Breakout | Volume > 150% of 20-day avg | Confirmed breakout — take the trade |
| Breakout | Volume < average | Failed breakout likely — wait or fade |
| Volume climax | Extreme volume spike (3x+ average) | Potential exhaustion/reversal point |

---

### Support & Resistance

**Fibonacci Retracement Levels:**
```
After a move from Low (L) to High (H):
  23.6% level = H - (H - L) * 0.236
  38.2% level = H - (H - L) * 0.382
  50.0% level = H - (H - L) * 0.500
  61.8% level = H - (H - L) * 0.618  (Golden Ratio — strongest level)
  78.6% level = H - (H - L) * 0.786

Worked example (move from $80 to $120):
  Range = $40
  23.6% = 120 - 40 * 0.236 = 120 - 9.44  = $110.56
  38.2% = 120 - 40 * 0.382 = 120 - 15.28 = $104.72
  50.0% = 120 - 40 * 0.500 = 120 - 20.00 = $100.00
  61.8% = 120 - 40 * 0.618 = 120 - 24.72 = $95.28  (most likely bounce)
  78.6% = 120 - 40 * 0.786 = 120 - 31.44 = $88.56
```

**Pivot Points (Standard):**
```
PP = (High + Low + Close) / 3
S1 = 2 * PP - High
S2 = PP - (High - Low)
R1 = 2 * PP - Low
R2 = PP + (High - Low)

Worked example (prev day: High=155, Low=148, Close=152):
  PP = (155 + 148 + 152) / 3 = 151.67
  S1 = 2 * 151.67 - 155 = 148.33
  S2 = 151.67 - (155 - 148) = 144.67
  R1 = 2 * 151.67 - 148 = 155.33
  R2 = 151.67 + (155 - 148) = 158.67
```

---

## 2. Candlestick Patterns

### Single-Candle Patterns
| Pattern | Signal | Body | Wicks | Context Required |
|---------|--------|------|-------|------------------|
| Doji | Indecision | Open = Close (or nearly) | Long both sides | At S/R level = reversal |
| Hammer | Bullish reversal | Small, at top of candle | Lower wick > 2x body | Must appear at bottom of downtrend |
| Inverted Hammer | Bullish reversal | Small, at bottom of candle | Upper wick > 2x body | At bottom of downtrend, needs confirmation |
| Shooting Star | Bearish reversal | Small, at bottom of candle | Upper wick > 2x body | Must appear at top of uptrend |
| Hanging Man | Bearish reversal | Small, at top of candle | Lower wick > 2x body | At top of uptrend (same shape as Hammer) |
| Marubozu (Bullish) | Strong continuation | Full green body, no wicks | None | Strong buying pressure |
| Marubozu (Bearish) | Strong continuation | Full red body, no wicks | None | Strong selling pressure |
| Spinning Top | Indecision | Small body centered | Equal wicks both sides | Trend may be losing steam |
| Dragonfly Doji | Bullish reversal | Open = Close = High | Long lower wick only | At support = strong reversal signal |
| Gravestone Doji | Bearish reversal | Open = Close = Low | Long upper wick only | At resistance = strong reversal signal |

### Multi-Candle Patterns
| Pattern | Signal | Description | Reliability |
|---------|--------|-------------|-------------|
| Bullish Engulfing | Reversal up | Large green candle fully engulfs prior red candle | High at support |
| Bearish Engulfing | Reversal down | Large red candle fully engulfs prior green candle | High at resistance |
| Morning Star | Bullish reversal | Red candle, small body/doji with gap, large green candle | Very High |
| Evening Star | Bearish reversal | Green candle, small body/doji with gap, large red candle | Very High |
| Three White Soldiers | Strong bullish | Three consecutive large green candles, each closing higher | Very High |
| Three Black Crows | Strong bearish | Three consecutive large red candles, each closing lower | Very High |
| Bullish Harami | Potential reversal | Large red, then small green contained within red's body | Moderate (needs confirmation) |
| Bearish Harami | Potential reversal | Large green, then small red contained within green's body | Moderate (needs confirmation) |
| Tweezer Bottom | Bullish reversal | Two candles with matching lows at support | High |
| Tweezer Top | Bearish reversal | Two candles with matching highs at resistance | High |
| Piercing Line | Bullish reversal | Red candle, then green opens below red's low and closes above 50% of red's body | Moderate-High |
| Dark Cloud Cover | Bearish reversal | Green candle, then red opens above green's high and closes below 50% of green's body | Moderate-High |

---

## 3. Risk Management Formulas

### Position Sizing (Fixed Fractional)
```
Position Size (shares) = Account Risk Amount / (Entry Price - Stop Loss Price)
Account Risk Amount    = Portfolio Value * Risk Per Trade %

RULE: Never risk more than 1-2% of portfolio on a single trade.

Worked example:
  Portfolio Value = $10,000
  Risk Per Trade  = 2% ($200)
  Entry Price     = $100.00
  Stop Loss       = $95.00 (based on 2x ATR below entry)
  Risk per share  = $100.00 - $95.00 = $5.00
  Position Size   = $200 / $5.00 = 40 shares
  Position Value  = 40 * $100 = $4,000 (40% of portfolio)

  CONCENTRATION CHECK: If position value > 10% of portfolio, reduce size.
  Adjusted: max position = $1,000 / $100 = 10 shares
  Adjusted risk = 10 * $5.00 = $50 (only 0.5% of portfolio — acceptable)
```

### Kelly Criterion (Optimal Bet Size)
```
Kelly % = W - ((1 - W) / R)
Where:
  W = win rate (decimal)
  R = average win / average loss ratio (reward-to-risk)

Worked example:
  Win rate: 60% (W = 0.60)
  Average win: $300, Average loss: $200
  R = 300 / 200 = 1.5
  Kelly = 0.60 - (0.40 / 1.5) = 0.60 - 0.267 = 0.333 (33.3%)

  Full Kelly is too aggressive for real trading. Use fractions:
  Half-Kelly  = 0.333 / 2 = 16.7% of portfolio per trade
  Quarter-Kelly = 0.333 / 4 = 8.3% of portfolio per trade (recommended)

  If Kelly is negative, the system has NEGATIVE expectancy — do not trade it.
```

### Value at Risk (VaR)
```
Parametric VaR = Portfolio Value * Portfolio Volatility * Z-score * sqrt(Time Horizon)

Z-scores:  90% confidence = 1.282
           95% confidence = 1.645
           99% confidence = 2.326

Worked example (daily VaR, 95% confidence):
  Portfolio = $10,000
  Daily volatility (stddev of daily returns) = 2.0%
  VaR = $10,000 * 0.02 * 1.645 * sqrt(1) = $329.00
  Meaning: 95% confident daily loss will not exceed $329.

  Weekly VaR = $329 * sqrt(5) = $329 * 2.236 = $735.65
  Monthly VaR = $329 * sqrt(21) = $329 * 4.583 = $1,507.81
```

### Sharpe Ratio
```
Sharpe = (Rp - Rf) / StdDev(Rp) * sqrt(252)
Where:
  Rp = mean daily portfolio return
  Rf = daily risk-free rate (Treasury yield / 252)
  StdDev(Rp) = standard deviation of daily returns
  252 = trading days per year (annualization factor)

Worked example:
  Mean daily return = 0.10% (0.001)
  Annual Treasury yield = 5.0% -> daily Rf = 0.05/252 = 0.000198
  StdDev of daily returns = 0.80% (0.008)
  Daily Sharpe = (0.001 - 0.000198) / 0.008 = 0.100
  Annualized Sharpe = 0.100 * sqrt(252) = 0.100 * 15.875 = 1.59

  Ratings:
    < 0.5  = Poor (not compensated for risk)
    0.5-1.0 = Acceptable
    1.0-2.0 = Good
    2.0-3.0 = Very Good
    > 3.0   = Excellent (verify — may indicate overfitting)
```

### Sortino Ratio (Downside-Only Risk)
```
Sortino = (Rp - Rf) / DownsideDeviation * sqrt(252)
DownsideDeviation = sqrt(mean(min(Ri - Rf, 0)^2))

Better than Sharpe because it only penalizes downside volatility, not upside.
Sortino > 2.0 is considered very good.
```

### Maximum Drawdown
```
For each point t in equity curve:
  Peak(t)     = max(Equity[0..t])
  Drawdown(t) = (Peak(t) - Equity(t)) / Peak(t) * 100%
  MaxDrawdown = max(Drawdown(t)) for all t

Worked example:
  Equity curve: $10,000 -> $12,000 -> $9,600 -> $11,500
  Peak at $12,000
  Drawdown at $9,600 = (12,000 - 9,600) / 12,000 = 20.0%
  Max Drawdown = 20.0%

Recovery Factor = Total Net Profit / Max Drawdown
  If total profit = $3,000, MaxDD = $2,400 -> RF = 3,000/2,400 = 1.25

Calmar Ratio = Annual Return / Max Drawdown
  If annual return = 25%, MaxDD = 20% -> Calmar = 1.25 (target > 1.0)
```

### Profit Factor
```
Profit Factor = Gross Winning Trades / Gross Losing Trades

Worked example:
  10 winning trades totaling $5,000
  8 losing trades totaling $3,200
  Profit Factor = 5,000 / 3,200 = 1.5625

  Ratings: < 1.0 = losing system, 1.0-1.5 = marginal, 1.5-2.0 = good,
           2.0-3.0 = very good, > 3.0 = excellent (verify with enough trades)
```

### Expectancy Per Trade
```
Expectancy = (Win% * AvgWin) - (Loss% * AvgLoss)

Worked example:
  Win rate: 55%, Average win: $150, Average loss: $100
  Expectancy = (0.55 * 150) - (0.45 * 100) = 82.50 - 45.00 = $37.50/trade
  Over 100 trades: expected profit = $3,750

  Minimum for a viable system: Expectancy > 0 with at least 30 sample trades.
```

### Risk/Reward Ratio
```
R:R = (Target Price - Entry Price) / (Entry Price - Stop Loss Price)

Worked example:
  Entry = $100, Stop = $95, Target = $112
  R:R = (112 - 100) / (100 - 95) = 12 / 5 = 2.4:1

  Minimum acceptable R:R = 1.5:1
  With 40% win rate and 2:1 R:R: Expectancy = 0.40*2 - 0.60*1 = +0.20 (profitable!)
  With 40% win rate and 1:1 R:R: Expectancy = 0.40*1 - 0.60*1 = -0.20 (losing!)
```

---

## 4. Alpaca Trading API Reference

### Authentication
```bash
# Paper trading (ALWAYS start here)
BASE_URL="https://paper-api.alpaca.markets"

# Live trading (only after paper validation)
# BASE_URL="https://api.alpaca.markets"

# Data API (same for both paper and live)
DATA_URL="https://data.alpaca.markets"

# Auth headers (required on every request)
HEADERS="-H 'APCA-API-KEY-ID: $ALPACA_API_KEY' -H 'APCA-API-SECRET-KEY: $ALPACA_SECRET_KEY'"
```

### Account Information
```bash
# Get account details
curl -s "$BASE_URL/v2/account" $HEADERS
# Key fields: id, status, equity, cash, buying_power, portfolio_value,
#   pattern_day_trader (bool), daytrade_count, last_equity
```

### Get Current Positions
```bash
# All positions
curl -s "$BASE_URL/v2/positions" $HEADERS
# Returns array: symbol, qty, side, avg_entry_price, current_price,
#   unrealized_pl, unrealized_plpc, market_value, cost_basis

# Single position
curl -s "$BASE_URL/v2/positions/AAPL" $HEADERS
```

### Place Orders
```bash
# Market order (fills immediately at best available price)
curl -s -X POST "$BASE_URL/v2/orders" $HEADERS \
  -H "Content-Type: application/json" \
  -d '{"symbol":"AAPL","qty":"10","side":"buy","type":"market","time_in_force":"day"}'

# Limit order (fills only at your price or better)
curl -s -X POST "$BASE_URL/v2/orders" $HEADERS \
  -H "Content-Type: application/json" \
  -d '{"symbol":"AAPL","qty":"10","side":"buy","type":"limit","time_in_force":"gtc","limit_price":"150.00"}'

# Stop order (triggers market order when stop price hit)
curl -s -X POST "$BASE_URL/v2/orders" $HEADERS \
  -H "Content-Type: application/json" \
  -d '{"symbol":"AAPL","qty":"10","side":"sell","type":"stop","time_in_force":"gtc","stop_price":"145.00"}'

# Stop-limit order (triggers limit order when stop price hit)
curl -s -X POST "$BASE_URL/v2/orders" $HEADERS \
  -H "Content-Type: application/json" \
  -d '{"symbol":"AAPL","qty":"10","side":"sell","type":"stop_limit","time_in_force":"gtc","stop_price":"145.00","limit_price":"144.50"}'

# Trailing stop (dynamic stop that trails price by dollar or percent amount)
curl -s -X POST "$BASE_URL/v2/orders" $HEADERS \
  -H "Content-Type: application/json" \
  -d '{"symbol":"AAPL","qty":"10","side":"sell","type":"trailing_stop","time_in_force":"gtc","trail_percent":"5"}'

# Bracket order (entry + stop loss + take profit as one atomic order)
curl -s -X POST "$BASE_URL/v2/orders" $HEADERS \
  -H "Content-Type: application/json" \
  -d '{
    "symbol": "AAPL",
    "qty": "10",
    "side": "buy",
    "type": "limit",
    "time_in_force": "day",
    "limit_price": "150.00",
    "order_class": "bracket",
    "stop_loss": {"stop_price": "145.00"},
    "take_profit": {"limit_price": "165.00"}
  }'

# OCO order (one-cancels-other: stop loss OR take profit, whichever hits first)
curl -s -X POST "$BASE_URL/v2/orders" $HEADERS \
  -H "Content-Type: application/json" \
  -d '{
    "symbol": "AAPL",
    "qty": "10",
    "side": "sell",
    "type": "limit",
    "time_in_force": "gtc",
    "limit_price": "165.00",
    "order_class": "oco",
    "stop_loss": {"stop_price": "145.00"}
  }'
```

**Order parameters reference:**
| Parameter | Values | Notes |
|-----------|--------|-------|
| `side` | `buy`, `sell` | |
| `type` | `market`, `limit`, `stop`, `stop_limit`, `trailing_stop` | |
| `time_in_force` | `day`, `gtc`, `ioc`, `fok` | day = cancel at close, gtc = good til canceled |
| `order_class` | `simple`, `bracket`, `oco`, `oto` | bracket = entry + stop + target |
| `qty` | String number | Whole shares for stocks |
| `notional` | String dollar amount | Alternative to qty (fractional shares) |

### Manage Orders
```bash
# List open orders
curl -s "$BASE_URL/v2/orders?status=open" $HEADERS

# Get specific order
curl -s "$BASE_URL/v2/orders/{order_id}" $HEADERS

# Cancel specific order
curl -s -X DELETE "$BASE_URL/v2/orders/{order_id}" $HEADERS

# Cancel ALL open orders
curl -s -X DELETE "$BASE_URL/v2/orders" $HEADERS
```

### Close Positions
```bash
# Close entire position in a symbol
curl -s -X DELETE "$BASE_URL/v2/positions/AAPL" $HEADERS

# Partially close (sell 5 of 10 shares)
curl -s -X DELETE "$BASE_URL/v2/positions/AAPL?qty=5" $HEADERS

# EMERGENCY: Close ALL positions
curl -s -X DELETE "$BASE_URL/v2/positions" $HEADERS
```

### Market Data (free with Alpaca account)
```bash
# Latest quote (bid/ask)
curl -s "$DATA_URL/v2/stocks/AAPL/quotes/latest" $HEADERS

# Latest trade (last fill)
curl -s "$DATA_URL/v2/stocks/AAPL/trades/latest" $HEADERS

# Historical bars (OHLCV) — daily
curl -s "$DATA_URL/v2/stocks/AAPL/bars?timeframe=1Day&start=2024-01-01&limit=100" $HEADERS

# Intraday bars — 5-minute
curl -s "$DATA_URL/v2/stocks/AAPL/bars?timeframe=5Min&start=$(date -d 'today' +%Y-%m-%d)&limit=78" $HEADERS

# Multi-symbol snapshot
curl -s "$DATA_URL/v2/stocks/snapshots?symbols=AAPL,MSFT,GOOGL" $HEADERS

# Crypto bars
curl -s "$DATA_URL/v1beta3/crypto/us/bars?symbols=BTC/USD&timeframe=1Day&limit=30" $HEADERS

# Crypto latest quote
curl -s "$DATA_URL/v1beta3/crypto/us/latest/quotes?symbols=BTC/USD,ETH/USD" $HEADERS
```

### Market Clock & Calendar
```bash
# Is market open right now?
curl -s "$BASE_URL/v2/clock" $HEADERS
# Returns: timestamp, is_open (bool), next_open, next_close

# Upcoming market calendar
curl -s "$BASE_URL/v2/calendar?start=$(date +%Y-%m-%d)&end=$(date -d '+7 days' +%Y-%m-%d)" $HEADERS
```

### Crypto Trading Notes
- Symbols use slash format: `BTC/USD`, `ETH/USD`, `SOL/USD`, `DOGE/USD`
- 24/7 trading (no market hours restriction)
- Fractional quantities allowed (e.g., `"qty": "0.001"` for BTC)
- Paper trading works identically to live
- Use `notional` for dollar-based crypto orders: `"notional": "100.00"` buys $100 worth

### Account Activity & History
```bash
# Trade history
curl -s "$BASE_URL/v2/account/activities/FILL?after=2024-01-01" $HEADERS

# Portfolio history
curl -s "$BASE_URL/v2/account/portfolio/history?period=1M&timeframe=1D" $HEADERS
# Returns: timestamp[], equity[], profit_loss[], profit_loss_pct[]
```

---

## 5. Free Financial Data Sources

### Price Data (via web_search + web_fetch)
| Source | URL Pattern | Data Available |
|--------|-------------|----------------|
| Yahoo Finance | `finance.yahoo.com/quote/AAPL` | Realtime quotes, charts, financials, analyst ratings |
| Google Finance | `google.com/finance/quote/AAPL:NASDAQ` | Quotes, news, related stocks, earnings |
| CoinGecko | `coingecko.com/en/coins/bitcoin` | Crypto prices, market cap, volume, 24h change |
| CoinMarketCap | `coinmarketcap.com/currencies/bitcoin/` | Crypto prices, rankings, dominance, supply |
| MarketWatch | `marketwatch.com/investing/stock/AAPL` | Quotes, news, analysis, options data |
| Finviz | `finviz.com/quote.ashx?t=AAPL` | Technical + fundamental screener, charts |
| TradingView | `tradingview.com/symbols/NASDAQ-AAPL/` | Charts, technicals, community ideas |

### Fundamental Data
| Source | URL Pattern | Data Available |
|--------|-------------|----------------|
| Macrotrends | `macrotrends.net/stocks/charts/AAPL/apple/pe-ratio` | P/E, revenue, margins, historical |
| Simply Wall St | Web search: `"AAPL simply wall st"` | Visual fundamental analysis, fair value |
| SEC EDGAR | `sec.gov/cgi-bin/browse-edgar?action=getcompany&CIK=AAPL&type=10-K` | Official 10-K, 10-Q, 8-K filings |
| Earnings Whispers | `earningswhispers.com/stocks/AAPL` | Earnings estimates, surprise history, calendar |
| Stock Analysis | `stockanalysis.com/stocks/AAPL/financials/` | Clean financial statements, ratios |
| Wisesheets | Web search: `"AAPL income statement"` | Financial data in spreadsheet format |

### Sentiment & Alternative Data
| Source | URL | Data Available |
|--------|-----|----------------|
| CNN Fear & Greed | `money.cnn.com/data/fear-and-greed/` | Market sentiment index 0-100 (Extreme Fear to Extreme Greed) |
| CBOE VIX | Web search: `"VIX index today"` | Volatility index (>30 = fear, <15 = complacency) |
| Finviz Map | `finviz.com/map.ashx` | Market heatmap by sector/size |
| StockTwits | `stocktwits.com/symbol/AAPL` | Social sentiment (bullish/bearish ratio) |
| Put/Call Ratio | Web search: `"CBOE put call ratio today"` | Options sentiment (>1.0 = bearish, <0.7 = bullish) |
| Short Interest | `finviz.com/quote.ashx?t=AAPL` -> Short Float | Percent of float sold short |
| Insider Trading | `openinsider.com/screener` | CEO/CFO buy/sell patterns |

### Macro Economic Data
| Source | URL | Data Available |
|--------|-----|----------------|
| FRED | `fred.stlouisfed.org` | Interest rates, CPI, employment, GDP, M2, yield curve |
| Treasury.gov | `treasury.gov/resource-center/data-chart-center/interest-rates/` | Daily Treasury yield curve |
| CME FedWatch | Web search: `"CME FedWatch tool"` | Federal funds rate probabilities |
| BLS | `bls.gov/news.release/` | Employment situation, CPI, PPI |
| ISM | Web search: `"ISM manufacturing PMI"` | PMI (>50 = expansion, <50 = contraction) |
| Conference Board | Web search: `"consumer confidence index"` | Consumer confidence, leading indicators |
| Earnings Calendar | `earningswhispers.com/calendar` | Upcoming earnings dates |
| Economic Calendar | Web search: `"economic calendar this week"` | Scheduled data releases |

### Crypto-Specific Sources
| Source | URL | Data Available |
|--------|-----|----------------|
| CoinGecko | `coingecko.com` | Prices, market cap, volume, DeFi TVL |
| DefiLlama | `defillama.com` | Total Value Locked across all chains |
| Glassnode (free tier) | Web search: `"bitcoin on-chain metrics"` | On-chain analytics (NUPL, MVRV, exchange flows) |
| Bitcoin Fear & Greed | `alternative.me/crypto/fear-and-greed-index/` | Crypto-specific sentiment 0-100 |
| Ultrasound Money | `ultrasound.money` | ETH supply/burn metrics |

---

## 6. Confidence Calibration Guide (Superforecasting)

### Calibration Principles (Philip Tetlock)
- A "70% confident" prediction should be right about 70% of the time
- Most people are overconfident: their "90%" predictions are right only ~70%
- Track your predictions systematically and compare predicted vs actual frequency
- Update incrementally (2-5% per new piece of evidence), not dramatically

### Confidence Level Guide
| Level | Meaning | Evidence Required | Trading Action |
|-------|---------|-------------------|----------------|
| 20-30% | Slight lean | Single weak signal, limited data | No trade — insufficient edge |
| 40-50% | Toss-up with slight edge | Conflicting signals, moderate evidence | No trade — coin flip |
| 55-65% | Moderate conviction | Multiple aligned signals, historical precedent | Small position, wide stops |
| 70-80% | Strong conviction | Strong multi-factor alignment, catalyst identified | Standard position size |
| 85-95% | Very high conviction | Overwhelming evidence — be suspicious of yourself | Full position, but NEVER all-in |

### Brier Score for Trade Predictions
```
Brier Score = mean((predicted_probability - actual_outcome)^2)
actual_outcome: 1 if prediction was correct, 0 if wrong

Worked example (5 predictions):
  Pred 1: 80% confident -> correct (1)  -> (0.80 - 1)^2 = 0.04
  Pred 2: 60% confident -> wrong (0)    -> (0.60 - 0)^2 = 0.36
  Pred 3: 70% confident -> correct (1)  -> (0.70 - 1)^2 = 0.09
  Pred 4: 90% confident -> correct (1)  -> (0.90 - 1)^2 = 0.01
  Pred 5: 55% confident -> wrong (0)    -> (0.55 - 0)^2 = 0.30
  Brier Score = (0.04 + 0.36 + 0.09 + 0.01 + 0.30) / 5 = 0.16

  Ratings: 0.00 = perfect, < 0.15 = excellent, 0.15-0.25 = good,
           0.25 = coin flip, > 0.25 = worse than random
```

### Calibration Self-Check Protocol
After accumulating 20+ trade predictions, group by confidence bucket:
1. Are your 60% predictions right ~60% of the time?
2. If your 60% predictions are right 80% of the time, you are underconfident — adjust up
3. If your 80% predictions are right 55% of the time, you are overconfident — adjust down
4. Recalibrate your confidence scale after every 50 resolved predictions

---

## 7. Trading Psychology & Cognitive Biases

### Biases to Watch For
| Bias | Description | Mitigation |
|------|-------------|------------|
| **Confirmation Bias** | Seeking info that confirms your thesis | Always build the opposing case first (adversarial debate) |
| **Anchoring** | Over-weighting the first number you see (entry price, analyst target) | Start analysis from base rates and current data, not old prices |
| **Recency Bias** | Over-weighting recent events (last week's crash, last month's rally) | Look at longer timeframes — 6-month and 1-year charts minimum |
| **Loss Aversion** | Holding losers too long ("it'll come back"), cutting winners too fast | Use mechanical stop-losses and take-profit targets, set BEFORE entry |
| **Overconfidence** | Believing you are more right than you are | Track Brier scores, use Kelly fractions, never bet > 2% per trade |
| **Narrative Bias** | Compelling story = good trade (often false) | Focus on quantitative data, not stories. "Good company" != "good trade" |
| **FOMO** | Fear of missing out, chasing entries | Only enter at planned levels. The market is open 252 days a year |
| **Sunk Cost** | "I've lost so much, I can't sell now" | Each moment is a new decision. Ask: "Would I enter this trade NOW at current price?" |
| **Hindsight Bias** | "I knew that would happen" | Journal BEFORE trades with specific predictions, not after |
| **Disposition Effect** | Selling winners early to "lock in profits" but holding losers | Let winners run (trail stops), cut losers at planned stops |
| **Gambler's Fallacy** | "It's dropped 5 days in a row, it HAS to bounce" | Each day is independent. Trends persist more often than they reverse |
| **Endowment Effect** | Overvaluing positions you already own | Evaluate positions as if you were building from scratch today |

### Discipline Rules
1. Every trade has a written plan BEFORE entry: entry price, stop loss, target, position size, thesis
2. Write down your reasoning BEFORE entering — if you cannot articulate the edge, do not trade
3. Set stop-losses at order entry time, not "in your head"
4. Review your journal weekly — look for patterns in wins AND losses
5. Take breaks after big wins (overconfidence risk) AND big losses (emotional risk)
6. Never average down on a losing position unless the original thesis explicitly planned for it
7. Never move a stop-loss further away from your entry (only tighten, never widen)
8. The market will be there tomorrow — missing a trade is not a loss, but a blown account is

---

## 8. Portfolio Construction

### Asset Allocation Guidelines
| Style | Equities | Crypto | Fixed Income / Cash | Max Single Position |
|-------|----------|--------|---------------------|---------------------|
| Conservative | 50-60% | 0-5% | 35-50% | 5% |
| Moderate | 60-75% | 5-15% | 10-35% | 8% |
| Aggressive | 70-85% | 10-25% | 5-20% | 10% |
| Speculative | 50-70% | 20-40% | 5-10% | 15% (with strict stops) |

### Sector Diversification
Maximum 30% in any single sector:
- Technology, Healthcare, Financials, Consumer Discretionary, Consumer Staples
- Energy, Industrials, Utilities, Real Estate, Materials, Communication Services

### Correlation Awareness
Highly correlated positions amplify risk. Check correlations before adding:
| Pair | Typical Correlation | Risk |
|------|---------------------|------|
| AAPL + MSFT + GOOGL | 0.7-0.9 | Concentrated large-cap tech |
| BTC + ETH + SOL | 0.8-0.95 | Concentrated crypto (moves together) |
| SPY + QQQ | 0.9+ | Nearly identical exposure |
| Stocks + Bonds | -0.2 to 0.3 | Genuinely diversifying |
| Gold + Stocks | -0.1 to 0.2 | Hedge in crisis |
| VIX + SPY | -0.8 | Inverse — VIX as hedge |

### Rebalancing Rules
- **Calendar**: Rebalance quarterly (first trading day of quarter)
- **Threshold**: Rebalance when any allocation drifts > 5% from target
- **Tax-aware**: Prefer rebalancing via new contributions rather than selling (taxable accounts)

---

## 9. Cross-Platform Commands

### Windows (PowerShell / Git Bash)
```bash
# Python might be `python` not `python3` on Windows
python -c "import json; ..."

# Use forward slashes in file paths or escape backslashes
# curl is available via Git Bash, PowerShell, or WSL

# Check if market is open (Windows Git Bash)
curl -s "$BASE_URL/v2/clock" -H "APCA-API-KEY-ID: $ALPACA_API_KEY" \
  -H "APCA-API-SECRET-KEY: $ALPACA_SECRET_KEY" | python -c "
import sys, json
d = json.load(sys.stdin)
print('OPEN' if d['is_open'] else 'CLOSED', '| Next:', d.get('next_open','') or d.get('next_close',''))
"
```

### macOS / Linux
```bash
python3 -c "import json; ..."
# curl, jq typically available by default
# Use jq for JSON processing:
curl -s URL | jq '.equity'
```

### JSON Processing Without jq
```bash
# Pretty-print JSON
python3 -c "import sys,json; print(json.dumps(json.load(sys.stdin),indent=2))" < file.json

# Extract specific field
curl -s URL | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['equity'])"

# Parse Alpaca positions into readable table
curl -s "$BASE_URL/v2/positions" $HEADERS | python3 -c "
import sys, json
positions = json.load(sys.stdin)
fmt = '{:<8} {:>6} {:>10} {:>10} {:>12} {:>8}'
print(fmt.format('Symbol','Qty','Entry','Current','P/L','P/L pct'))
print('-' * 60)
for p in positions:
    print(fmt.format(p['symbol'], p['qty'], float(p['avg_entry_price']),
          float(p['current_price']), float(p['unrealized_pl']),
          round(float(p['unrealized_plpc'])*100,2)))
"

# Calculate RSI from historical bars
curl -s "$DATA_URL/v2/stocks/AAPL/bars?timeframe=1Day&limit=30" $HEADERS | python3 -c "
import sys, json
data = json.load(sys.stdin)
closes = [float(b['c']) for b in data['bars']]
changes = [closes[i]-closes[i-1] for i in range(1, len(closes))]
gains = [max(c,0) for c in changes[-14:]]
losses = [abs(min(c,0)) for c in changes[-14:]]
avg_gain = sum(gains)/14
avg_loss = sum(losses)/14
rs = avg_gain/avg_loss if avg_loss > 0 else 999
rsi = 100 - (100/(1+rs))
print(f'RSI(14) = {rsi:.1f}')
"
```

---

## 10. Pre-Trade Checklist

Before every trade, verify ALL of the following:

```
PRE-TRADE CHECKLIST
====================
[ ] 1. TREND: What is the higher-timeframe trend? (Daily chart 200 SMA)
       - Trading WITH the trend? (preferred)
       - Counter-trend? (requires stronger signal + tighter stops)

[ ] 2. SIGNAL: What specific setup triggered this trade?
       - Indicator signal (RSI, MACD, etc.)
       - Pattern (candlestick, chart pattern)
       - Catalyst (earnings, news, sector rotation)

[ ] 3. ENTRY: Exact entry price or condition
       - Limit order at specific level? Market order on breakout?

[ ] 4. STOP LOSS: Exact stop price
       - Based on ATR (2-3x ATR from entry)
       - Below key support (long) or above key resistance (short)
       - NEVER wider than 2% of portfolio

[ ] 5. TARGET: Exact take-profit price
       - Risk/Reward at least 1.5:1 (preferably 2:1+)
       - At logical resistance (long) or support (short)

[ ] 6. POSITION SIZE: Calculated from risk management rules
       - Risk amount = Portfolio * 1-2%
       - Shares = Risk amount / (Entry - Stop)
       - Total position < 10% of portfolio

[ ] 7. CORRELATION CHECK: Does this overlap with existing positions?
       - Not adding to concentrated sector exposure
       - Total portfolio heat (sum of open risk) < 6%

[ ] 8. CATALYST CHECK: Any upcoming events that could gap through stops?
       - Earnings date? Fed meeting? CPI release?
       - If yes: reduce size or wait until after event

[ ] 9. MARKET CONTEXT: Is the overall market favorable?
       - Fear & Greed index level
       - VIX level (>30 = caution, <15 = complacency risk)
       - Market trend (SPY vs 200 SMA)

[ ] 10. CONFIDENCE: Rate 1-10 honestly
        - Below 6? Skip the trade
        - Record confidence for calibration tracking
```

---

## 11. Trade Journal Template

```json
{
  "trade_id": "T001",
  "date_opened": "2025-01-15",
  "date_closed": null,
  "symbol": "AAPL",
  "side": "long",
  "entry_price": 150.00,
  "stop_loss": 145.00,
  "target": 162.00,
  "position_size": 40,
  "risk_amount": 200.00,
  "risk_reward": 2.4,
  "setup": "Bullish engulfing at 50 EMA + RSI divergence",
  "confidence": 7,
  "market_context": "SPY above 200 SMA, VIX at 18, F&G neutral (52)",
  "pre_trade_thesis": "AAPL pulled back to 50 EMA support, RSI showing bullish divergence, earnings in 3 weeks should provide catalyst. Sector (tech) is leading.",
  "result": {
    "exit_price": null,
    "exit_reason": null,
    "pnl": null,
    "pnl_percent": null,
    "held_days": null,
    "lessons": null
  }
}
```

Store trade journals using `memory_store` for tracking and calibration review.

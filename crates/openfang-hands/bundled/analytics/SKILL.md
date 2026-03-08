---
name: analytics-hand-skill
version: "1.0.0"
description: "Expert knowledge for data analysis — pandas operations, visualization recipes, statistical methods, KPI frameworks, data cleaning, and reporting templates"
runtime: prompt_only
---

# Data Analysis Expert Knowledge

## Python Pandas Cheat Sheet

### Loading Data

```python
import pandas as pd

# CSV
df = pd.read_csv("file.csv")
df = pd.read_csv("file.csv", parse_dates=["date_col"], index_col="id")
df = pd.read_csv("file.csv", dtype={"col": str}, na_values=["N/A", "null", ""])

# JSON
df = pd.read_json("file.json")
df = pd.json_normalize(nested_dict, record_path="items", meta=["id", "name"])

# Excel
df = pd.read_excel("file.xlsx", sheet_name="Sheet1")

# SQL
import sqlite3
conn = sqlite3.connect("db.sqlite")
df = pd.read_sql("SELECT * FROM table_name", conn)

# From dictionary
df = pd.DataFrame({"col1": [1, 2, 3], "col2": ["a", "b", "c"]})

# Clipboard (interactive)
df = pd.read_clipboard()
```

### Filtering & Selection

```python
# Column selection
df["col"]                         # Single column (Series)
df[["col1", "col2"]]              # Multiple columns (DataFrame)

# Row filtering
df[df["age"] > 30]                                # Boolean mask
df[(df["age"] > 30) & (df["city"] == "NYC")]      # Multiple conditions (& = AND)
df[(df["status"] == "A") | (df["status"] == "B")] # OR condition
df[df["name"].str.contains("John", na=False)]     # String contains
df[df["col"].isin(["val1", "val2"])]              # In list
df[df["col"].between(10, 50)]                     # Range
df.query("age > 30 and city == 'NYC'")            # Query syntax
df[df["col"].notna()]                             # Not null
df.nlargest(10, "revenue")                        # Top N
df.nsmallest(5, "cost")                           # Bottom N
```

### Grouping & Aggregation

```python
# Basic groupby
df.groupby("category")["revenue"].sum()
df.groupby("category")["revenue"].agg(["mean", "median", "std", "count"])

# Multiple groupby columns
df.groupby(["year", "category"])["revenue"].sum()

# Named aggregation (pandas 0.25+)
df.groupby("category").agg(
    total_rev=("revenue", "sum"),
    avg_rev=("revenue", "mean"),
    count=("id", "count"),
    max_date=("date", "max")
)

# Transform (returns same-shaped result)
df["pct_of_group"] = df.groupby("category")["revenue"].transform(lambda x: x / x.sum())

# Rolling aggregation
df["rolling_7d_avg"] = df["metric"].rolling(window=7).mean()
df["cumulative_sum"] = df["revenue"].cumsum()
```

### Pivot Tables

```python
# Pivot table
pd.pivot_table(df, values="revenue", index="region", columns="product",
               aggfunc="sum", fill_value=0, margins=True)

# Cross tabulation
pd.crosstab(df["category"], df["status"], normalize="index")  # Row percentages
```

### Merge & Join

```python
# Inner join
merged = pd.merge(df1, df2, on="id", how="inner")

# Left join
merged = pd.merge(df1, df2, on="id", how="left")

# Join on different column names
merged = pd.merge(df1, df2, left_on="user_id", right_on="id")

# Multiple join keys
merged = pd.merge(df1, df2, on=["year", "category"])

# Concatenate vertically
combined = pd.concat([df1, df2], ignore_index=True)

# Concatenate horizontally
combined = pd.concat([df1, df2], axis=1)
```

### Date Operations

```python
df["date"] = pd.to_datetime(df["date_str"])
df["year"] = df["date"].dt.year
df["month"] = df["date"].dt.month
df["day_of_week"] = df["date"].dt.day_name()
df["quarter"] = df["date"].dt.quarter
df["days_since"] = (pd.Timestamp.now() - df["date"]).dt.days

# Resample time series
df.set_index("date").resample("W")["revenue"].sum()   # Weekly sum
df.set_index("date").resample("M")["users"].mean()    # Monthly average
```

---

## Matplotlib Visualization Recipes

### Line Chart

```python
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

fig, ax = plt.subplots(figsize=(12, 5))
ax.plot(df["date"], df["revenue"], color="#2196F3", linewidth=2, label="Revenue")
ax.plot(df["date"], df["target"], color="#FF5722", linewidth=1, linestyle="--", label="Target")
ax.fill_between(df["date"], df["revenue"], alpha=0.1, color="#2196F3")
ax.set_title("Monthly Revenue vs Target", fontsize=14, fontweight="bold")
ax.set_xlabel("Date")
ax.set_ylabel("Revenue ($)")
ax.legend()
ax.grid(True, alpha=0.3)
plt.xticks(rotation=45)
plt.tight_layout()
plt.savefig("chart_line.png", dpi=150)
plt.close()
```

### Bar Chart

```python
fig, ax = plt.subplots(figsize=(10, 6))
categories = df["category"].value_counts()
bars = ax.bar(categories.index, categories.values, color="#4CAF50", edgecolor="black", alpha=0.8)

# Add value labels on bars
for bar in bars:
    height = bar.get_height()
    ax.text(bar.get_x() + bar.get_width() / 2., height,
            f'{height:,.0f}', ha='center', va='bottom', fontsize=10)

ax.set_title("Count by Category", fontsize=14, fontweight="bold")
ax.set_xlabel("Category")
ax.set_ylabel("Count")
plt.xticks(rotation=45)
plt.tight_layout()
plt.savefig("chart_bar.png", dpi=150)
plt.close()
```

### Grouped Bar Chart

```python
import numpy as np

categories = df["category"].unique()
x = np.arange(len(categories))
width = 0.35

fig, ax = plt.subplots(figsize=(12, 6))
ax.bar(x - width/2, df.groupby("category")["metric1"].mean(), width, label="Metric 1", color="#2196F3")
ax.bar(x + width/2, df.groupby("category")["metric2"].mean(), width, label="Metric 2", color="#FF9800")
ax.set_xticks(x)
ax.set_xticklabels(categories, rotation=45)
ax.legend()
ax.set_title("Comparison by Category")
plt.tight_layout()
plt.savefig("chart_grouped_bar.png", dpi=150)
plt.close()
```

### Scatter Plot

```python
fig, ax = plt.subplots(figsize=(8, 8))
scatter = ax.scatter(df["x"], df["y"], c=df["color_metric"], cmap="viridis",
                     s=50, alpha=0.6, edgecolors="black", linewidth=0.5)
plt.colorbar(scatter, label="Color Metric")
ax.set_title("X vs Y")
ax.set_xlabel("X Variable")
ax.set_ylabel("Y Variable")

# Add trend line
z = np.polyfit(df["x"], df["y"], 1)
p = np.poly1d(z)
ax.plot(sorted(df["x"]), p(sorted(df["x"])), "r--", alpha=0.8, label=f"Trend (y={z[0]:.2f}x+{z[1]:.2f})")
ax.legend()
plt.tight_layout()
plt.savefig("chart_scatter.png", dpi=150)
plt.close()
```

### Heatmap

```python
fig, ax = plt.subplots(figsize=(10, 8))
corr = df.select_dtypes(include=[np.number]).corr()
im = ax.imshow(corr, cmap="RdBu_r", vmin=-1, vmax=1, aspect="auto")

# Add text annotations
for i in range(len(corr)):
    for j in range(len(corr)):
        text = ax.text(j, i, f"{corr.iloc[i, j]:.2f}",
                       ha="center", va="center", fontsize=9,
                       color="white" if abs(corr.iloc[i, j]) > 0.5 else "black")

ax.set_xticks(range(len(corr.columns)))
ax.set_yticks(range(len(corr.columns)))
ax.set_xticklabels(corr.columns, rotation=45, ha="right")
ax.set_yticklabels(corr.columns)
plt.colorbar(im, label="Correlation")
ax.set_title("Correlation Heatmap")
plt.tight_layout()
plt.savefig("chart_heatmap.png", dpi=150)
plt.close()
```

### Histogram with KDE

```python
fig, ax = plt.subplots(figsize=(10, 6))
ax.hist(df["metric"], bins=30, density=True, alpha=0.7, color="#2196F3", edgecolor="black", label="Distribution")

# Add KDE line
from scipy.stats import gaussian_kde
kde = gaussian_kde(df["metric"].dropna())
x_range = np.linspace(df["metric"].min(), df["metric"].max(), 200)
ax.plot(x_range, kde(x_range), color="#FF5722", linewidth=2, label="KDE")

# Add mean/median lines
ax.axvline(df["metric"].mean(), color="red", linestyle="--", label=f"Mean: {df['metric'].mean():.2f}")
ax.axvline(df["metric"].median(), color="green", linestyle="--", label=f"Median: {df['metric'].median():.2f}")

ax.set_title("Distribution of Metric")
ax.set_xlabel("Value")
ax.set_ylabel("Density")
ax.legend()
plt.tight_layout()
plt.savefig("chart_histogram.png", dpi=150)
plt.close()
```

---

## Plotly Interactive Chart Recipes

### Interactive Time Series

```python
import plotly.express as px
import plotly.io as pio

fig = px.line(df, x="date", y="revenue", color="category",
              title="Revenue Over Time by Category",
              labels={"revenue": "Revenue ($)", "date": "Date"})
fig.update_layout(hovermode="x unified")
pio.write_html(fig, "chart_timeseries_interactive.html")
pio.write_image(fig, "chart_timeseries.png", scale=2)
```

### Interactive Scatter with Trendline

```python
fig = px.scatter(df, x="cost", y="revenue", size="units", color="category",
                 trendline="ols", hover_data=["name"],
                 title="Revenue vs Cost by Category")
pio.write_html(fig, "chart_scatter_interactive.html")
pio.write_image(fig, "chart_scatter.png", scale=2)
```

### Funnel Chart

```python
import plotly.graph_objects as go

stages = ["Visitors", "Signups", "Activated", "Paid", "Retained"]
values = [10000, 3200, 1800, 600, 420]

fig = go.Figure(go.Funnel(y=stages, x=values,
                          textinfo="value+percent initial+percent previous"))
fig.update_layout(title="Conversion Funnel")
pio.write_html(fig, "chart_funnel.html")
pio.write_image(fig, "chart_funnel.png", scale=2)
```

### Subplots Dashboard

```python
from plotly.subplots import make_subplots
import plotly.graph_objects as go

fig = make_subplots(rows=2, cols=2,
                    subplot_titles=("Revenue Trend", "Category Split",
                                    "Monthly Growth", "Top Products"))

fig.add_trace(go.Scatter(x=df["date"], y=df["revenue"], name="Revenue"), row=1, col=1)
fig.add_trace(go.Pie(labels=cats, values=vals, name="Categories"), row=1, col=2)
fig.add_trace(go.Bar(x=months, y=growth, name="Growth %"), row=2, col=1)
fig.add_trace(go.Bar(x=products, y=product_rev, name="Products"), row=2, col=2)

fig.update_layout(height=800, title_text="Analytics Dashboard")
pio.write_html(fig, "dashboard.html")
```

---

## Statistical Analysis Reference

### Descriptive Statistics

| Measure | Function | When to Use |
|---------|----------|------------|
| Mean | `df["col"].mean()` | Central tendency (normal distribution) |
| Median | `df["col"].median()` | Central tendency (skewed data) |
| Mode | `df["col"].mode()` | Most common value (categorical) |
| Std Dev | `df["col"].std()` | Spread (how dispersed values are) |
| Variance | `df["col"].var()` | Spread (squared units) |
| Skewness | `df["col"].skew()` | Distribution asymmetry (>1 or <-1 = highly skewed) |
| Kurtosis | `df["col"].kurtosis()` | Tail heaviness (>3 = heavy tails) |
| IQR | `df["col"].quantile(0.75) - df["col"].quantile(0.25)` | Robust spread measure |
| Coefficient of Variation | `df["col"].std() / df["col"].mean()` | Relative variability |

### Correlation

```python
from scipy import stats

# Pearson (linear relationship, normally distributed)
r, p = stats.pearsonr(df["x"], df["y"])

# Spearman (monotonic relationship, any distribution)
rho, p = stats.spearmanr(df["x"], df["y"])

# Kendall (ordinal data, small samples)
tau, p = stats.kendalltau(df["x"], df["y"])
```

**Interpretation of r/rho**:
```
|r| < 0.1   Negligible
0.1 - 0.3   Weak
0.3 - 0.5   Moderate
0.5 - 0.7   Strong
0.7 - 0.9   Very strong
> 0.9       Near perfect
```

### Hypothesis Testing

```python
from scipy import stats

# t-test: compare two group means
t_stat, p_val = stats.ttest_ind(group_a, group_b)
# p < 0.05 → statistically significant difference

# Chi-squared: test independence of categorical variables
contingency = pd.crosstab(df["cat1"], df["cat2"])
chi2, p, dof, expected = stats.chi2_contingency(contingency)

# Mann-Whitney U: non-parametric alternative to t-test
u_stat, p_val = stats.mannwhitneyu(group_a, group_b, alternative="two-sided")

# ANOVA: compare means across 3+ groups
f_stat, p_val = stats.f_oneway(group_a, group_b, group_c)
```

### Regression Basics

```python
from scipy import stats
import numpy as np

# Simple linear regression
slope, intercept, r_value, p_value, std_err = stats.linregress(x, y)
r_squared = r_value ** 2
print(f"y = {slope:.4f}x + {intercept:.4f}")
print(f"R-squared: {r_squared:.4f}")
print(f"p-value: {p_value:.6f}")

# Predictions
predicted = slope * x_new + intercept

# Multiple regression (use statsmodels)
import statsmodels.api as sm
X = sm.add_constant(df[["x1", "x2", "x3"]])
model = sm.OLS(df["y"], X).fit()
print(model.summary())
```

---

## KPI Frameworks

### North Star Metric

The single metric that best captures the core value your product delivers.

```
Framework:
  1. What is the core value your product delivers?
  2. What action signals a user received that value?
  3. How frequently should that action occur?

Examples:
  Spotify → Time spent listening (weekly)
  Airbnb → Nights booked
  Slack → Messages sent per team per day
  Shopify → Gross Merchant Volume (GMV)
```

### HEART Framework (Google)

| Dimension | Definition | Signal | Metric |
|-----------|-----------|--------|--------|
| **Happiness** | User satisfaction | Survey, NPS, ratings | NPS score, CSAT |
| **Engagement** | Depth of interaction | Actions per session, frequency | DAU/MAU, sessions/user |
| **Adoption** | New user uptake | Signups, first action | Activation rate, new users/week |
| **Retention** | Users coming back | Return visits, renewals | D7/D30 retention, churn rate |
| **Task Success** | Efficiency completing goals | Time to complete, error rate | Completion rate, time-on-task |

### OKR Structure

```
Objective: [Qualitative goal — what you want to achieve]
  KR1: [Quantitative result] — [current] → [target] by [date]
  KR2: [Quantitative result] — [current] → [target] by [date]
  KR3: [Quantitative result] — [current] → [target] by [date]

Example:
  Objective: Improve user onboarding experience
    KR1: Activation rate 35% → 55% by Q2
    KR2: Time to first value 4.2 days → 1.5 days by Q2
    KR3: Day-7 retention 22% → 35% by Q2
```

---

## Data Cleaning Patterns

### Handling Missing Values (NaN)

```python
# Detect
df.isnull().sum()                          # Count nulls per column
df.isnull().sum() / len(df) * 100          # Percentage null

# Strategy by missing percentage
# < 5%:  Drop rows or impute with median/mode
# 5-30%: Impute with mean/median/mode or predictive imputation
# > 30%: Consider dropping column or using indicator variable

# Imputation
df["numeric_col"].fillna(df["numeric_col"].median(), inplace=True)    # Median (robust)
df["category_col"].fillna(df["category_col"].mode()[0], inplace=True) # Mode
df["col"].fillna(method="ffill", inplace=True)                        # Forward fill (time series)

# Indicator variable for missingness
df["col_was_missing"] = df["col"].isnull().astype(int)
```

### Type Conversion

```python
# String to numeric
df["col"] = pd.to_numeric(df["col"], errors="coerce")  # Invalid → NaN

# String to datetime
df["date"] = pd.to_datetime(df["date_str"], format="%Y-%m-%d", errors="coerce")

# Numeric to category
df["bucket"] = pd.cut(df["age"], bins=[0, 18, 35, 50, 65, 100],
                       labels=["<18", "18-35", "35-50", "50-65", "65+"])

# Boolean conversion
df["active"] = df["status"].map({"active": True, "inactive": False})
```

### Outlier Detection

```python
import numpy as np

# IQR method (standard)
Q1, Q3 = df["col"].quantile(0.25), df["col"].quantile(0.75)
IQR = Q3 - Q1
lower, upper = Q1 - 1.5 * IQR, Q3 + 1.5 * IQR
outliers = df[(df["col"] < lower) | (df["col"] > upper)]

# Z-score method (assumes normal distribution)
from scipy import stats
z_scores = np.abs(stats.zscore(df["col"].dropna()))
outliers = df[z_scores > 3]  # Beyond 3 standard deviations

# Decision: remove, cap, or keep with flag
df["col_capped"] = df["col"].clip(lower=lower, upper=upper)  # Cap at bounds
df["is_outlier"] = ((df["col"] < lower) | (df["col"] > upper)).astype(int)  # Flag
```

---

## Common Analytical Patterns

### Cohort Analysis

```python
# Define cohort by first action month
df["cohort"] = df.groupby("user_id")["date"].transform("min").dt.to_period("M")
df["period"] = df["date"].dt.to_period("M")
df["cohort_age"] = (df["period"] - df["cohort"]).apply(lambda x: x.n)

# Build cohort table
cohort_table = df.groupby(["cohort", "cohort_age"])["user_id"].nunique().unstack()

# Retention rates
cohort_sizes = cohort_table[0]
retention = cohort_table.divide(cohort_sizes, axis=0).round(3)
print("Retention Table:")
print(retention)

# Visualize
import seaborn as sns
fig, ax = plt.subplots(figsize=(12, 8))
sns.heatmap(retention, annot=True, fmt=".0%", cmap="YlGn", ax=ax)
ax.set_title("Cohort Retention Analysis")
ax.set_xlabel("Months Since First Action")
ax.set_ylabel("Cohort (First Month)")
plt.tight_layout()
plt.savefig("chart_cohort_retention.png", dpi=150)
```

### Funnel Analysis

```python
# Define funnel stages and count users at each
stages = {
    "Visited": df["visited"].sum(),
    "Signed Up": df["signed_up"].sum(),
    "Activated": df["activated"].sum(),
    "Purchased": df["purchased"].sum(),
    "Retained (D30)": df["retained_d30"].sum(),
}

funnel = pd.DataFrame({
    "Stage": stages.keys(),
    "Users": stages.values(),
})
funnel["Conversion"] = (funnel["Users"] / funnel["Users"].iloc[0] * 100).round(1)
funnel["Step Rate"] = (funnel["Users"] / funnel["Users"].shift(1) * 100).round(1)
funnel["Drop-off"] = (100 - funnel["Step Rate"]).round(1)

print(funnel.to_string(index=False))
# Biggest drop-off = biggest optimization opportunity
```

### A/B Test Analysis

```python
from scipy import stats
import numpy as np

# Sample data
control = df[df["variant"] == "control"]["metric"]
treatment = df[df["variant"] == "treatment"]["metric"]

# Summary
print(f"Control:   n={len(control)}, mean={control.mean():.4f}, std={control.std():.4f}")
print(f"Treatment: n={len(treatment)}, mean={treatment.mean():.4f}, std={treatment.std():.4f}")

# Lift
lift = (treatment.mean() - control.mean()) / control.mean() * 100
print(f"Lift: {lift:.2f}%")

# Statistical significance (two-sample t-test)
t_stat, p_value = stats.ttest_ind(control, treatment)
print(f"t-statistic: {t_stat:.4f}")
print(f"p-value: {p_value:.6f}")
print(f"Significant at 95%: {'YES' if p_value < 0.05 else 'NO'}")

# Confidence interval for the difference
diff = treatment.mean() - control.mean()
se = np.sqrt(control.var() / len(control) + treatment.var() / len(treatment))
ci_low = diff - 1.96 * se
ci_high = diff + 1.96 * se
print(f"95% CI for difference: [{ci_low:.4f}, {ci_high:.4f}]")

# Effect size (Cohen's d)
pooled_std = np.sqrt((control.std()**2 + treatment.std()**2) / 2)
cohens_d = diff / pooled_std
print(f"Cohen's d: {cohens_d:.4f} ({'small' if abs(cohens_d) < 0.5 else 'medium' if abs(cohens_d) < 0.8 else 'large'})")

# Sample size check (was the test properly powered?)
from scipy.stats import norm
alpha = 0.05
power = 0.8
min_n = (2 * ((norm.ppf(1 - alpha/2) + norm.ppf(power)) * pooled_std / diff) ** 2)
print(f"Min sample size needed: {int(min_n)} per group")
print(f"Actual: {min(len(control), len(treatment))} per group")
```

### Period-over-Period Comparison

```python
# Month-over-month comparison
current = df[df["month"] == current_month]
previous = df[df["month"] == previous_month]

comparison = pd.DataFrame({
    "Metric": ["Revenue", "Users", "Conversion", "Avg Order Value"],
    "Current": [current["revenue"].sum(), current["user_id"].nunique(),
                current["converted"].mean(), current["order_value"].mean()],
    "Previous": [previous["revenue"].sum(), previous["user_id"].nunique(),
                 previous["converted"].mean(), previous["order_value"].mean()],
})
comparison["Change"] = comparison["Current"] - comparison["Previous"]
comparison["Change %"] = ((comparison["Change"] / comparison["Previous"]) * 100).round(1)
comparison["Direction"] = comparison["Change"].apply(lambda x: "UP" if x > 0 else "DOWN" if x < 0 else "FLAT")
print(comparison.to_string(index=False))
```

---

## Report Template Structure

### Executive Report (1 page)

```markdown
# [Title] Analysis Report
**Period**: [date range] | **Prepared**: [date] | **Analyst**: Analytics Hand

## Key Metrics
| Metric | Value | vs Previous | Trend |
|--------|-------|------------|-------|
| [KPI 1] | [value] | [+/-X%] | [arrow] |
| [KPI 2] | [value] | [+/-X%] | [arrow] |
| [KPI 3] | [value] | [+/-X%] | [arrow] |

## Top 3 Insights
1. **[Insight headline]** — [one sentence with specific numbers]
2. **[Insight headline]** — [one sentence with specific numbers]
3. **[Insight headline]** — [one sentence with specific numbers]

## Recommended Actions
1. [Action] — expected impact: [estimate]
2. [Action] — expected impact: [estimate]

## Charts
[Inline chart images]
```

### Deep-Dive Report

```markdown
# [Title] Deep-Dive Analysis
**Period**: [date range] | **Dataset**: [description] | **Records**: [N]

## Executive Summary
[2-3 sentences summarizing key findings and recommendations]

## Methodology
- Data source: [description]
- Cleaning: [steps taken]
- Analysis type: [descriptive/diagnostic/predictive]
- Tools: [pandas, matplotlib, scipy]

## Data Quality Assessment
- Records: [total] | After cleaning: [total]
- Missing data: [summary]
- Outliers: [summary]

## Findings

### 1. [Finding Title]
[Detailed explanation with numbers, charts, and statistical backing]
![chart](chart_name.png)

### 2. [Finding Title]
[Detailed explanation]

## Statistical Tests
| Test | Variables | Statistic | p-value | Conclusion |
|------|----------|-----------|---------|------------|
| [test] | [vars] | [value] | [p] | [significant?] |

## Limitations
- [Limitation 1]
- [Limitation 2]

## Appendix
- [Raw tables, additional charts, code snippets]
```

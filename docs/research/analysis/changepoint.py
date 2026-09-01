#!/usr/bin/env python3
"""Registered steady-state detection for benchmark-protocol.md (rev 3 as
amended, section 4).

PELT on log per-iteration times: ruptures, cost l2, min_size 3,
penalty = 3 * sigma2_hat * log(n), where sigma2_hat is the robust
first-difference variance of the log times,
    sigma2_hat = (MAD(diff(y)) / 0.6745)^2 / 2,
the /2 correcting the variance doubling of differencing. This is the
BIC-style scaling required for the l2 cost (which has units of log-value^2);
an unscaled penalty cannot detect realistic warmups. Degenerate guard for
coarse timer quantisation (ties): if MAD is zero, fall back to the plain
variance of the differences / 2; if that is zero too the series is constant
and is steady by definition.

Steady state = the final segment, which must have >= 5 iterations and
|Kendall tau| <= 0.4 (registered trend gate). Only pre-final-segment
iterations are discarded.

Usage: uv run changepoint.py '<json array of per-iteration seconds>'
Emits JSON: {"steady": bool, "start_index": int, "discarded": int,
             "kendall_tau": float, "sigma2_hat": float, "reason": str|null}
"""

import json
import sys

import numpy as np
import ruptures as rpt
from scipy import stats

MIN_FINAL = 5
TAU_GATE = 0.4


def sigma2_hat(y):
    d = np.diff(y)
    mad = np.median(np.abs(d - np.median(d)))
    if mad > 0:
        return float((mad / 0.6745) ** 2 / 2)
    return float(np.var(d) / 2)


def analyse(times):
    n = len(times)
    y = np.log(np.asarray(times, dtype=float))
    s2 = sigma2_hat(y)
    if s2 == 0.0:  # constant series (timer quantisation): steady by definition
        return {"steady": True, "start_index": 0, "discarded": 0,
                "kendall_tau": 0.0, "sigma2_hat": 0.0, "reason": None}
    penalty = 3.0 * s2 * np.log(n)
    algo = rpt.Pelt(model="l2", min_size=3).fit(y.reshape(-1, 1))
    breaks = algo.predict(pen=penalty)  # includes n as final boundary
    start = 0 if len(breaks) <= 1 else breaks[-2]
    final = y[start:]
    if len(final) < MIN_FINAL:
        return {"steady": False, "start_index": int(start),
                "discarded": int(start), "kendall_tau": None, "sigma2_hat": s2,
                "reason": f"final segment has {len(final)} < {MIN_FINAL} iterations"}
    tau = stats.kendalltau(np.arange(len(final)), final).statistic
    if abs(tau) > TAU_GATE:
        return {"steady": False, "start_index": int(start),
                "discarded": int(start), "kendall_tau": float(tau),
                "sigma2_hat": s2,
                "reason": f"|kendall tau| {abs(tau):.3f} > {TAU_GATE}"}
    return {"steady": True, "start_index": int(start), "discarded": int(start),
            "kendall_tau": float(tau), "sigma2_hat": s2, "reason": None}


if __name__ == "__main__":
    print(json.dumps(analyse(json.loads(sys.argv[1]))))

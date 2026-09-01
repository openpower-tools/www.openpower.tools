#!/usr/bin/env python3
"""Registered Bayesian analysis for benchmark-protocol.md (REGISTERED 2026-09-02, rev 3).

Input: JSONL, one row per repetition:
    {"cell": str, "block": int, "run": int, "metric": str, "value": float}
`value` is tok/s for rate metrics (pp, tg, embed) or seconds for durations
(ttft); everything is modelled on the log scale either way.

Modes:
    fit    — single-cell posterior + threshold probabilities (D1/D2/D4).
        uv run analyze.py fit data.jsonl --metric tg --cell N4 \
            --guess 30 --thresholds 3,4 --seed 1
    ratio  — paired two-cell ratio with SHARED block effects (D3, A/A).
        uv run analyze.py ratio data.jsonl --metric tg \
            --cell-a A16-on --cell-b A16-off --rope 1.02 --seed 1
    selftest — synthetic-data recovery check of the pipeline.

REGISTERED PARAMETERS (do not change without a protocol amendment commit):
    priors: mu ~ Normal(log guess, 1.0); tau_b, tau_r ~ HalfNormal(0.10);
            sigma_w ~ HalfNormal(0.05); nu ~ 1 + Gamma(alpha=2, RATE beta=0.1)
    sampling: 4 chains, tune 2000, draws 2000, target_accept 0.95
    gates: R-hat <= 1.01; bulk ESS >= 1000; tail ESS >= 400;
           MCSE(decision P) <= 0.005; zero divergences
    interval: 94% HDI on the log scale (exponentiated, labelled), plus the
              equal-tailed 3%..97% interval on the natural scale
    sensitivity suite: {tau_* ~ HN(0.2)}, {sigma_w ~ HN(0.1)},
              {mu sd 2.0}, {nu ~ 1 + Exponential(scale=29)} — every decision
              verdict must be invariant across the suite
    fresh-run predictive: a NEW RUN-LEVEL MEAN = mu + eps_b*tau_b + eps_r*tau_r
--smoke shrinks sampling for pipeline testing ONLY; smoke output is never a
reportable result and is labelled as such.
"""

import argparse
import json
import sys

import arviz as az
import numpy as np
import pymc as pm
from scipy import stats

HDI_PROB = 0.94
GATES = {"rhat": 1.01, "ess_bulk": 1000, "ess_tail": 400, "mcse_p": 0.005}
SENSITIVITY = [
    {"name": "wider-tau", "tau_sd": 0.20},
    {"name": "wider-sigw", "sigw_sd": 0.10},
    {"name": "wider-mu", "mu_sd": 2.0},
    {"name": "nu-exp", "nu_exp": True},
]


def load(path, metric, cell):
    rows = []
    with open(path) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            if row["metric"] == metric and row["cell"] == cell:
                rows.append(row)
    if not rows:
        sys.exit(f"no rows for metric={metric} cell={cell}")
    y = np.log([r["value"] for r in rows])
    blocks = sorted({r["block"] for r in rows})
    runs = sorted({(r["block"], r["run"]) for r in rows})
    b_of = {b: i for i, b in enumerate(blocks)}
    r_of = {br: i for i, br in enumerate(runs)}
    b_idx = np.array([b_of[r["block"]] for r in rows])
    r_idx = np.array([r_of[(r["block"], r["run"])] for r in rows])
    run_block = np.array([b_of[br[0]] for br in runs])
    return y, b_idx, r_idx, len(blocks), len(runs), run_block


def build(y, b_idx, r_idx, n_b, n_r, guess, alt=None):
    alt = alt or {}
    with pm.Model() as model:
        mu = pm.Normal("mu", mu=np.log(guess), sigma=alt.get("mu_sd", 1.0))
        tau_b = pm.HalfNormal("tau_b", alt.get("tau_sd", 0.10))
        tau_r = pm.HalfNormal("tau_r", alt.get("tau_sd", 0.10))
        zb = pm.Normal("zb", 0, 1, shape=n_b)
        zr = pm.Normal("zr", 0, 1, shape=n_r)
        sigma_w = pm.HalfNormal("sigma_w", alt.get("sigw_sd", 0.05))
        if alt.get("nu_exp"):
            nu = pm.Deterministic("nu", 1 + pm.Exponential("nu_raw", lam=1 / 29))
        else:
            nu = pm.Deterministic("nu", 1 + pm.Gamma("nu_raw", alpha=2.0, beta=0.1))
        loc = mu + zb[b_idx] * tau_b + zr[r_idx] * tau_r
        pm.StudentT("obs", nu=nu, mu=loc, sigma=sigma_w, observed=y)
    return model


def sample(model, seed, smoke):
    draws, tune = (200, 200) if smoke else (2000, 2000)
    with model:
        idata = pm.sample(
            draws=draws, tune=tune, chains=4, target_accept=0.95,
            random_seed=seed, progressbar=False,
        )
    return idata


def gate_report(idata, names):
    summ = az.summary(idata, var_names=names, ci_prob=HDI_PROB)
    div = int(idata.sample_stats.diverging.values.sum())
    ok = bool(
        (summ["r_hat"] <= GATES["rhat"]).all()
        and (summ["ess_bulk"] >= GATES["ess_bulk"]).all()
        and (summ["ess_tail"] >= GATES["ess_tail"]).all()
        and div == 0
    )
    return ok, {"divergences": div, "summary": summ.to_dict()}


def decision_p(samples, threshold_log, n_eff):
    p = float((samples >= threshold_log).mean())
    mcse = float(np.sqrt(max(p * (1 - p), 1e-12) / max(n_eff, 1.0)))
    return p, mcse


def fresh_run_draws(idata, rng):
    post = idata.posterior
    mu = post["mu"].values.ravel()
    tau_b = post["tau_b"].values.ravel()
    tau_r = post["tau_r"].values.ravel()
    return mu + tau_b * rng.standard_normal(mu.shape) + tau_r * rng.standard_normal(mu.shape)


def conjugate_check(y, r_idx, n_r, threshold_log):
    run_means = np.array([y[r_idx == i].mean() for i in range(n_r)])
    m, s = run_means.mean(), run_means.std(ddof=1)
    if s == 0 or n_r < 3:
        return None
    t = (threshold_log - m) / (s / np.sqrt(n_r))
    return {"p_mu_ge": float(stats.t.sf(t, df=n_r - 1)), "run_mean": float(np.exp(m))}


def summarize_fit(idata, thresholds, seed):
    post = idata.posterior
    mu = post["mu"].values.ravel()
    ess = float(az.ess(idata, var_names=["mu"])["mu"].values)
    hdi = az.hdi(idata, var_names=["mu"], prob=HDI_PROB)["mu"].values
    rng = np.random.default_rng(seed)
    fresh = fresh_run_draws(idata, rng)
    out = {
        "median": float(np.exp(np.median(mu))),
        "hdi94_log_exp": [float(np.exp(hdi[0])), float(np.exp(hdi[1]))],
        "eti94": [float(np.exp(np.quantile(mu, 0.03))), float(np.exp(np.quantile(mu, 0.97)))],
        "tau_b_pct": float((np.exp(np.median(post["tau_b"].values)) - 1) * 100),
        "tau_r_pct": float((np.exp(np.median(post["tau_r"].values)) - 1) * 100),
        "sigma_w_pct": float((np.exp(np.median(post["sigma_w"].values)) - 1) * 100),
        "nu_median": float(np.median(post["nu"].values)),
        "thresholds": {},
    }
    for x in thresholds:
        pt, mt = decision_p(mu, np.log(x), ess)
        pf, mf = decision_p(fresh, np.log(x), ess)
        out["thresholds"][str(x)] = {
            "p_typical_ge": pt, "mcse_typical": mt, "mcse_ok": mt <= GATES["mcse_p"],
            "p_fresh_run_ge": pf, "mcse_fresh": mf,
        }
    return out


def cmd_fit(args):
    y, b_idx, r_idx, n_b, n_r, _ = load(args.data, args.metric, args.cell)
    thresholds = [float(t) for t in args.thresholds.split(",")] if args.thresholds else []
    results = {"mode": "fit", "metric": args.metric, "cell": args.cell,
               "n": {"blocks": n_b, "runs": n_r, "reps": len(y)},
               "seed": args.seed, "smoke": args.smoke, "fits": {}}
    for alt in [None] + (SENSITIVITY if not args.smoke else []):
        name = alt["name"] if alt else "registered"
        idata = sample(build(y, b_idx, r_idx, n_b, n_r, args.guess, alt), args.seed, args.smoke)
        ok, gates = gate_report(idata, ["mu", "tau_b", "tau_r", "sigma_w", "nu"])
        fit = summarize_fit(idata, thresholds, args.seed)
        fit["gates_ok"] = ok
        fit["divergences"] = gates["divergences"]
        results["fits"][name] = fit
    if thresholds and len(results["fits"]) > 1:
        reg = results["fits"]["registered"]["thresholds"]
        invariant = all(
            (results["fits"][n]["thresholds"][t]["p_typical_ge"] >= 0.95) == (reg[t]["p_typical_ge"] >= 0.95)
            for n in results["fits"] for t in reg
        )
        results["sensitivity_invariant"] = invariant
    results["conjugate_check"] = (
        conjugate_check(y, r_idx, n_r, np.log(thresholds[0])) if thresholds else None
    )
    print(json.dumps(results, indent=2))


def cmd_ratio(args):
    ya, ba, ra, nba, nra, _ = load(args.data, args.metric, args.cell_a)
    yb, bb, rb, nbb, nrb, _ = load(args.data, args.metric, args.cell_b)
    if nba != nbb:
        sys.exit("paired ratio requires identical block sets in both cells")
    with pm.Model():
        mu_a = pm.Normal("mu_a", np.log(args.guess), 1.0)
        mu_b = pm.Normal("mu_b", np.log(args.guess), 1.0)
        tau_b = pm.HalfNormal("tau_b", 0.10)
        tau_r = pm.HalfNormal("tau_r", 0.10)
        zb = pm.Normal("zb", 0, 1, shape=nba)          # SHARED block effects
        zra = pm.Normal("zra", 0, 1, shape=nra)
        zrb = pm.Normal("zrb", 0, 1, shape=nrb)
        sigma_w = pm.HalfNormal("sigma_w", 0.05)
        nu = pm.Deterministic("nu", 1 + pm.Gamma("nu_raw", alpha=2.0, beta=0.1))
        pm.StudentT("oa", nu=nu, mu=mu_a + zb[ba] * tau_b + zra[ra] * tau_r,
                    sigma=sigma_w, observed=ya)
        pm.StudentT("ob", nu=nu, mu=mu_b + zb[bb] * tau_b + zrb[rb] * tau_r,
                    sigma=sigma_w, observed=yb)
        pm.Deterministic("delta", mu_a - mu_b)
        idata = pm.sample(draws=200 if args.smoke else 2000,
                          tune=200 if args.smoke else 2000, chains=4,
                          target_accept=0.95, random_seed=args.seed,
                          progressbar=False)
    delta = idata.posterior["delta"].values.ravel()
    rope = np.log(args.rope)
    hdi = az.hdi(idata, var_names=["delta"], prob=HDI_PROB)["delta"].values
    ess = float(az.ess(idata, var_names=["delta"])["delta"].values)
    ok, gates = gate_report(idata, ["mu_a", "mu_b", "tau_b", "tau_r", "sigma_w", "nu"])
    p_sup, mcse_sup = decision_p(delta, rope, ess)
    p_reg, _ = decision_p(-delta, rope, ess)  # P(delta < -rope)
    print(json.dumps({
        "mode": "ratio", "metric": args.metric,
        "cells": [args.cell_a, args.cell_b], "seed": args.seed, "smoke": args.smoke,
        "ratio_median": float(np.exp(np.median(delta))),
        "hdi94_log_exp": [float(np.exp(hdi[0])), float(np.exp(hdi[1]))],
        "p_ratio_gt_1": float((delta > 0).mean()),
        "p_superiority_beyond_rope": p_sup, "mcse": mcse_sup,
        "p_regression_beyond_rope": p_reg,
        "hdi_within_rope": bool(hdi[0] >= -rope and hdi[1] <= rope),
        "gates_ok": ok, "divergences": gates["divergences"],
    }, indent=2))


def cmd_selftest(args):
    rng = np.random.default_rng(7)
    rows = []
    for block in range(5):
        u = rng.normal(0, 0.03)
        for run in range(6):
            v = rng.normal(0, 0.05)
            for _ in range(8):
                val = float(np.exp(np.log(30.0) + u + v + rng.normal(0, 0.02)))
                rows.append({"cell": "SELF", "block": block, "run": run,
                             "metric": "tg", "value": val})
    path = "/tmp/analyze-selftest.jsonl"
    with open(path, "w") as fh:
        fh.write("\n".join(json.dumps(r) for r in rows))
    ns = argparse.Namespace(data=path, metric="tg", cell="SELF", guess=30.0,
                            thresholds="25,30", seed=1, smoke=True)
    cmd_fit(ns)
    print("SELFTEST-DONE (smoke sampling; recovery target: median near 30)",
          file=sys.stderr)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="mode", required=True)
    f = sub.add_parser("fit")
    f.add_argument("data")
    f.add_argument("--metric", required=True)
    f.add_argument("--cell", required=True)
    f.add_argument("--guess", type=float, required=True)
    f.add_argument("--thresholds", default="")
    f.add_argument("--seed", type=int, default=1)
    f.add_argument("--smoke", action="store_true")
    f.set_defaults(fn=cmd_fit)
    r = sub.add_parser("ratio")
    r.add_argument("data")
    r.add_argument("--metric", required=True)
    r.add_argument("--cell-a", required=True)
    r.add_argument("--cell-b", required=True)
    r.add_argument("--guess", type=float, default=30.0)
    r.add_argument("--rope", type=float, default=1.02)
    r.add_argument("--seed", type=int, default=1)
    r.add_argument("--smoke", action="store_true")
    r.set_defaults(fn=cmd_ratio)
    s = sub.add_parser("selftest")
    s.set_defaults(fn=cmd_selftest)
    args = p.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()

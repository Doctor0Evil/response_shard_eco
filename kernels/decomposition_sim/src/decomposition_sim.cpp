// File: kernels/decomposition_sim/src/decomposition_sim.cpp

#include <cmath>
#include <vector>

struct KineticsSample {
    double t_days;
    double mass_frac;        // remaining mass fraction m(t)/m0
    double leachate_tox;     // normalized toxicity 0–1
    double micro_residue;    // normalized micro-residue 0–1
};

struct DecompositionResult {
    double t90_days;         // time to 90% mass loss
    double r_t90;            // risk coord for t90
    double r_tox;            // risk coord for leachate toxicity
    double r_micro;          // risk coord for micro-residue
};

static double clamp01(double x) {
    return x < 0.0 ? 0.0 : (x > 1.0 ? 1.0 : x);
}

DecompositionResult simulate_first_order(
    const std::vector<KineticsSample>& samples,
    double t90_safe_max_days,
    double t90_hard_max_days
) {
    // Estimate t90 from samples: first time mass_frac ≤ 0.1
    double t90 = t90_hard_max_days;
    for (const auto& s : samples) {
        if (s.mass_frac <= 0.1) {
            t90 = s.t_days;
            break;
        }
    }

    // Normalize t90 into r_t90 (fast breakdown → low risk).
    double r_t90;
    if (t90 <= t90_safe_max_days) {
        r_t90 = 0.0;
    } else if (t90 >= t90_hard_max_days) {
        r_t90 = 1.0;
    } else {
        double span = t90_hard_max_days - t90_safe_max_days;
        double rel = (t90 - t90_safe_max_days) / span;
        r_t90 = clamp01(rel);
    }

    // Worst leachate toxicity and micro-residue across samples.
    double max_tox = 0.0;
    double max_micro = 0.0;
    for (const auto& s : samples) {
        if (s.leachate_tox > max_tox)  max_tox = s.leachate_tox;
        if (s.micro_residue > max_micro) max_micro = s.micro_residue;
    }

    DecompositionResult result;
    result.t90_days = t90;
    result.r_t90 = clamp01(r_t90);
    result.r_tox = clamp01(max_tox);
    result.r_micro = clamp01(max_micro);
    return result;
}

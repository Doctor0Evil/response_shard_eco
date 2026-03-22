// filename: cyboquatic-ecosafety-core/src/lib.rs
// destination: cyboquatic-ecosafety-core/src/lib.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Dimensionless risk coordinate in [0,1].
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RiskCoord(pub f64);

impl RiskCoord {
    pub fn clamped(x: f64) -> Self {
        RiskCoord(if x < 0.0 { 0.0 } else if x > 1.0 { 1.0 } else { x })
    }
}

/// Corridor bands for a single physical variable.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CorridorBands {
    pub safe: f64,
    pub gold: f64,
    pub hard: f64,
}

impl CorridorBands {
    /// Piecewise-linear normalization used across Cyboquatic, MAR, trays, etc.
    /// x <= safe   -> 0
    /// x >= hard   -> 1
    /// safe < x < gold: mild slope
    /// gold <= x < hard: steeper slope
    pub fn normalize(&self, x: f64) -> RiskCoord {
        let (safe, gold, hard) = (self.safe, self.gold, self.hard);
        if x <= safe {
            RiskCoord(0.0)
        } else if x >= hard {
            RiskCoord(1.0)
        } else if x < gold {
            let t = (x - safe) / (gold - safe).max(1e-12);
            RiskCoord::clamped(0.5 * t)
        } else {
            let t = (x - gold) / (hard - gold).max(1e-12);
            RiskCoord::clamped(0.5 + 0.5 * t)
        }
    }
}

/// Normalized risk vector for a node at one instant.
pub type RiskVector = HashMap<String, RiskCoord>;

/// Lyapunov residual V_t = Σ w_j r_j^2 with non-increase invariant.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ResidualState {
    pub vt: f64,
}

impl ResidualState {
    pub fn new() -> Self {
        ResidualState { vt: 0.0 }
    }

    pub fn update(&mut self, risks: &RiskVector, weights: &HashMap<String, f64>) -> f64 {
        let mut vt_new = 0.0;
        for (k, r) in risks {
            let w = *weights.get(k).unwrap_or(&1.0);
            vt_new += w * r.0 * r.0;
        }
        let vt_prev = self.vt;
        self.vt = vt_new;
        vt_prev
    }

    pub fn safestep_ok(&self, vt_prev: f64, eps: f64) -> bool {
        self.vt <= vt_prev + eps
    }
}

/// K/E/R triad computed from a time window.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct KerTriad {
    pub k_knowledge: f64,
    pub e_eco_impact: f64,
    pub r_risk_of_harm: f64,
}

/// Rolling window accumulator for K/E/R over a trajectory.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KerWindow {
    pub total_steps: u64,
    pub lyapunov_safe_steps: u64,
    pub max_risk: f64,
}

impl KerWindow {
    pub fn new() -> Self {
        KerWindow {
            total_steps: 0,
            lyapunov_safe_steps: 0,
            max_risk: 0.0,
        }
    }

    pub fn observe_step(&mut self, risks: &RiskVector, safestep_ok: bool) {
        self.total_steps += 1;
        if safestep_ok {
            self.lyapunov_safe_steps += 1;
        }
        for r in risks.values() {
            if r.0 > self.max_risk {
                self.max_risk = r.0;
            }
        }
    }

    pub fn finalize(&self) -> KerTriad {
        let k = if self.total_steps == 0 {
            0.0
        } else {
            self.lyapunov_safe_steps as f64 / self.total_steps as f64
        };
        let r = self.max_risk;
        let e = (1.0 - r).max(0.0).min(1.0);
        KerTriad {
            k_knowledge: k,
            e_eco_impact: e,
            r_risk_of_harm: r,
        }
    }
}

/// Trait implemented by any Cyboquatic industrial machine controller.
pub trait SafeController {
    /// Compute next actuation proposal and associated risk vector.
    fn propose_step(&self) -> (RiskVector, HashMap<String, f64>);

    /// Apply actuation if ecosafety kernel accepts it.
    fn apply_step(&mut self);
}

/// Ecosafety kernel implementing "no corridor, no build" and safestep gates.
pub struct EcoSafetyKernel {
    pub eps_vt: f64,
    pub residual: ResidualState,
}

impl EcoSafetyKernel {
    pub fn new(eps_vt: f64) -> Self {
        EcoSafetyKernel {
            eps_vt,
            residual: ResidualState::new(),
        }
    }

    /// Gate a controller step; returns true only if all invariants hold.
    pub fn evaluate_step<C: SafeController>(
        &mut self,
        controller: &mut C,
        window: &mut KerWindow,
    ) -> bool {
        let (risks, weights) = controller.propose_step();
        let vt_prev = self.residual.update(&risks, &weights);
        let ok = self.residual.safestep_ok(vt_prev, self.eps_vt);
        window.observe_step(&risks, ok);
        if ok {
            controller.apply_step();
        }
        ok
    }
}

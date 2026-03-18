// File: cyboquatic-ecosafety-core/src/lib.rs

#![forbid(unsafe_code)]

use std::fmt;

/// Dimensionless risk coordinate r ∈ [0,1].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RiskCoord(f64);

impl RiskCoord {
    pub fn new_clamped(v: f64) -> Self {
        let v = if v < 0.0 { 0.0 } else if v > 1.0 { 1.0 } else { v };
        RiskCoord(v)
    }
    pub fn value(self) -> f64 { self.0 }
}

/// Corridor bands for one physical metric.
#[derive(Clone, Copy, Debug)]
pub struct CorridorBands {
    pub x_safe: f64,
    pub x_gold: f64,
    pub x_hard: f64,
}

impl CorridorBands {
    pub fn normalize(self, x: f64) -> RiskCoord {
        if x <= self.x_safe {
            return RiskCoord::new_clamped(0.0);
        }
        if x >= self.x_hard {
            return RiskCoord::new_clamped(1.0);
        }
        if x <= self.x_gold {
            let num = x - self.x_safe;
            let den = (self.x_gold - self.x_safe).max(f64::EPSILON);
            return RiskCoord::new_clamped(num / den * 0.5);
        }
        let num = x - self.x_gold;
        let den = (self.x_hard - self.x_gold).max(f64::EPSILON);
        RiskCoord::new_clamped(0.5 + num / den * 0.5)
    }
}

/// Vector of risk coordinates.
#[derive(Clone, Debug)]
pub struct RiskVector {
    pub coords: Vec<RiskCoord>,
}

impl RiskVector {
    pub fn max(&self) -> RiskCoord {
        self.coords
            .iter()
            .copied()
            .max_by(|a, b| a.value().partial_cmp(&b.value()).unwrap())
            .unwrap_or(RiskCoord::new_clamped(0.0))
    }
}

/// Lyapunov residual V_t = Σ w_j r_j^2.
#[derive(Clone, Copy, Debug)]
pub struct Residual {
    pub vt: f64,
}

impl Residual {
    pub fn from_weights(risks: &RiskVector, weights: &[f64]) -> Self {
        let mut vt = 0.0;
        for (r, w) in risks.coords.iter().zip(weights.iter()) {
            let v = r.value();
            vt += w.max(0.0) * v * v;
        }
        Residual { vt }
    }
}

/// Rolling KER window.
#[derive(Clone, Copy, Debug)]
pub struct KerTriad {
    pub k_knowledge: f64,
    pub e_ecoimpact: f64,
    pub r_risk_of_harm: f64,
}

#[derive(Clone, Debug)]
pub struct KerWindow {
    total_steps: u64,
    lyapunov_safe_steps: u64,
    max_risk: f64,
}

impl KerWindow {
    pub fn new() -> Self {
        KerWindow {
            total_steps: 0,
            lyapunov_safe_steps: 0,
            max_risk: 0.0,
        }
    }

    pub fn update_step(&mut self, lyapunov_safe: bool, risks: &RiskVector) {
        self.total_steps += 1;
        if lyapunov_safe {
            self.lyapunov_safe_steps += 1;
        }
        let m = risks.max().value();
        if m > self.max_risk {
            self.max_risk = m;
        }
    }

    pub fn finalize(self) -> KerTriad {
        let k = if self.total_steps == 0 {
            1.0
        } else {
            self.lyapunov_safe_steps as f64 / self.total_steps as f64
        };
        let r = self.max_risk;
        let e = (1.0 - r).max(0.0);
        KerTriad {
            k_knowledge: k,
            e_ecoimpact: e,
            r_risk_of_harm: r,
        }
    }
}

/// Ecosafety kernel enforcing V_{t+1} ≤ V_t + ε and no hard-band violations.
pub struct EcoSafetyKernel {
    pub vt_prev: f64,
    pub eps_vt: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SafeStepDecision {
    Accept,
    Derate,
    Stop,
}

impl EcoSafetyKernel {
    pub fn new(eps_vt: f64) -> Self {
        EcoSafetyKernel { vt_prev: 0.0, eps_vt }
    }

    pub fn evaluate_step(&mut self, residual: Residual, risks: &RiskVector) -> SafeStepDecision {
        let vt = residual.vt;
        let max_r = risks.max().value();
        if max_r >= 1.0 {
            self.vt_prev = vt;
            return SafeStepDecision::Stop;
        }
        if vt <= self.vt_prev + self.eps_vt {
            self.vt_prev = vt;
            SafeStepDecision::Accept
        } else {
            self.vt_prev = vt;
            SafeStepDecision::Derate
        }
    }
}

/// Contract a controller must satisfy.
pub trait SafeController {
    type State;
    type Command;

    fn propose_step(&self, state: &Self::State) -> (Self::Command, RiskVector, Vec<f64>);
}

/// Gate that mediates between controller and actuators.
pub struct SafeStepGate<C: SafeController> {
    pub controller: C,
    pub kernel: EcoSafetyKernel,
}

impl<C: SafeController> SafeStepGate<C> {
    pub fn new(controller: C, kernel: EcoSafetyKernel) -> Self {
        SafeStepGate { controller, kernel }
    }

    pub fn next_step(&mut self, state: &C::State) -> (C::Command, SafeStepDecision) {
        let (cmd, risks, weights) = self.controller.propose_step(state);
        let residual = Residual::from_weights(&risks, &weights);
        let decision = self.kernel.evaluate_step(residual, &risks);
        (cmd, decision)
    }
}

impl fmt::Display for KerTriad {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "KER(k={:.3}, e={:.3}, r={:.3})",
            self.k_knowledge, self.e_ecoimpact, self.r_risk_of_harm
        )
    }
}

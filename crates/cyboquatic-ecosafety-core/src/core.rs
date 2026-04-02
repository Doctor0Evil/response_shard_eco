// filename: crates/cyboquatic-ecosafety-core/src/core.rs

#![no_std]

pub type RiskCoord = f32;          // clamped to [0.0, 1.0]
pub type LyapunovValue = f32;

#[derive(Clone, Copy)]
pub struct RiskVector {
    pub r_energy:       RiskCoord,
    pub r_hydraulics:   RiskCoord,
    pub r_biology:      RiskCoord,
    pub r_carbon:       RiskCoord,
    pub r_materials:    RiskCoord,
    pub r_biodiversity: RiskCoord,
}

#[derive(Clone, Copy)]
pub struct LyapunovWeights {
    pub w_energy:       f32,
    pub w_hydraulics:   f32,
    pub w_biology:      f32,
    pub w_carbon:       f32,
    pub w_materials:    f32,
    pub w_biodiversity: f32,
}

#[derive(Clone, Copy)]
pub struct Residual {
    pub v_t: LyapunovValue,
}

#[derive(Clone, Copy)]
pub struct KerWindow {
    pub k_safe_steps: u32,
    pub k_total_steps: u32,
    pub r_max: RiskCoord,
}

pub struct KerTriad {
    pub k_knowledge: f32,
    pub e_ecoimpact: f32,
    pub r_risk:      f32,
}

#[inline]
pub fn clamp01(x: f32) -> f32 {
    if x < 0.0 { 0.0 } else if x > 1.0 { 1.0 } else { x }
}

impl RiskVector {
    pub fn residual(&self, w: &LyapunovWeights) -> Residual {
        let s =
            w.w_energy       * self.r_energy       * self.r_energy +
            w.w_hydraulics   * self.r_hydraulics   * self.r_hydraulics +
            w.w_biology      * self.r_biology      * self.r_biology +
            w.w_carbon       * self.r_carbon       * self.r_carbon +
            w.w_materials    * self.r_materials    * self.r_materials +
            w.w_biodiversity * self.r_biodiversity * self.r_biodiversity;
        Residual { v_t: s }
    }

    pub fn r_max(&self) -> RiskCoord {
        let mut m = self.r_energy;
        if self.r_hydraulics   > m { m = self.r_hydraulics; }
        if self.r_biology      > m { m = self.r_biology; }
        if self.r_carbon       > m { m = self.r_carbon; }
        if self.r_materials    > m { m = self.r_materials; }
        if self.r_biodiversity > m { m = self.r_biodiversity; }
        m
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SafeDecision {
    Accept,
    Derate,
    Stop,
}

pub struct SafeStepConfig {
    pub epsilon: f32,     // allowed tiny increase around equilibrium
    pub v_interior: f32,  // small safe interior radius
}

pub fn safestep(
    prev: Residual,
    next: Residual,
    rv_next: &RiskVector,
    cfg: &SafeStepConfig,
) -> SafeDecision {
    // Hard corridor breach -> Stop
    if rv_next.r_max() >= 1.0 {
        return SafeDecision::Stop;
    }

    // Lyapunov non-increase outside small interior
    if prev.v_t > cfg.v_interior {
        if next.v_t > prev.v_t + cfg.epsilon {
            return SafeDecision::Derate;
        }
    }

    SafeDecision::Accept
}

impl KerWindow {
    pub fn new() -> Self {
        KerWindow {
            k_safe_steps:  0,
            k_total_steps: 0,
            r_max:         0.0,
        }
    }

    pub fn update(&mut self, decision: SafeDecision, rv: &RiskVector) {
        self.k_total_steps = self.k_total_steps.saturating_add(1);
        if matches!(decision, SafeDecision::Accept | SafeDecision::Derate) {
            self.k_safe_steps = self.k_safe_steps.saturating_add(1);
        }
        let r = rv.r_max();
        if r > self.r_max {
            self.r_max = r;
        }
    }

    pub fn triad(&self) -> KerTriad {
        let k = if self.k_total_steps == 0 {
            0.0
        } else {
            (self.k_safe_steps as f32) / (self.k_total_steps as f32)
        };
        let r = self.r_max;
        let e = 1.0 - r;
        KerTriad {
            k_knowledge: clamp01(k),
            e_ecoimpact: clamp01(e),
            r_risk:      clamp01(r),
        }
    }
}

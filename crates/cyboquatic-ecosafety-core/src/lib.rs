//! cyboquatic-ecosafety-core
//! Rust ecosafety spine for Cyboquatic industrial machinery (rx/Vt/KER).
//! All domains (MAR, FlowVac, FOG, wetlands, trays, air plenums) parameterize this grammar.

#![forbid(unsafe_code)]

use std::marker::PhantomData;
use std::time::{Duration, Instant};

pub type RiskScalar = f64;

/// Dimensionless risk coordinate r ∈ [0,1], clamped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RiskCoord(RiskScalar);

impl RiskCoord {
    pub fn new(raw: RiskScalar) -> Self {
        RiskCoord(raw.clamp(0.0, 1.0))
    }
    pub fn value(self) -> RiskScalar {
        self.0
    }
}

/// Corridor bands for a single physical metric (raw units, e.g., mg/L, m/d, days).
/// Safe ≤ Gold ≤ Hard.
#[derive(Clone, Copy, Debug)]
pub struct CorridorBands {
    pub safe: RiskScalar,
    pub gold: RiskScalar,
    pub hard: RiskScalar,
}

impl CorridorBands {
    pub fn assert_well_formed(&self) {
        assert!(
            self.safe <= self.gold && self.gold <= self.hard,
            "CorridorBands must satisfy safe ≤ gold ≤ hard"
        );
    }

    /// Piecewise-linear normalization into RiskCoord using safe/gold/hard bands.
    pub fn normalize(&self, x: RiskScalar) -> RiskCoord {
        self.assert_well_formed();

        if x <= self.safe {
            return RiskCoord::new(0.0);
        }
        if x >= self.hard {
            return RiskCoord::new(1.0);
        }

        if x <= self.gold {
            // Gentle slope between safe and gold.
            let num = x - self.safe;
            let den = (self.gold - self.safe).max(RiskScalar::EPSILON);
            return RiskCoord::new(num / den * 0.5);
        }

        // Steeper slope from gold to hard, mapping into (0.5, 1.0).
        let num = x - self.gold;
        let den = (self.hard - self.gold).max(RiskScalar::EPSILON);
        RiskCoord::new(0.5 + num / den * 0.5)
    }
}

/// Canonical planes for Cyboquatic machinery:
/// energy, hydraulics, biology, carbon, materials.
#[derive(Clone, Copy, Debug)]
pub struct RiskVector {
    pub energy: RiskCoord,
    pub hydraulics: RiskCoord,
    pub biology: RiskCoord,
    pub carbon: RiskCoord,
    pub materials: RiskCoord,
}

impl RiskVector {
    pub fn max_coord(&self) -> RiskCoord {
        let vals = [
            self.energy.value(),
            self.hydraulics.value(),
            self.biology.value(),
            self.carbon.value(),
            self.materials.value(),
        ];
        RiskCoord::new(vals.into_iter().fold(0.0, RiskScalar::max))
    }
}

/// Weights for Lyapunov residual over the five planes.
#[derive(Clone, Copy, Debug)]
pub struct ResidualWeights {
    pub w_energy: RiskScalar,
    pub w_hydraulics: RiskScalar,
    pub w_biology: RiskScalar,
    pub w_carbon: RiskScalar,
    pub w_materials: RiskScalar,
}

impl ResidualWeights {
    pub fn assert_non_negative(&self) {
        assert!(self.w_energy >= 0.0);
        assert!(self.w_hydraulics >= 0.0);
        assert!(self.w_biology >= 0.0);
        assert!(self.w_carbon >= 0.0);
        assert!(self.w_materials >= 0.0);
    }

    pub fn default_hazard_ordering() -> Self {
        Self {
            w_energy: 1.0,
            w_hydraulics: 1.2,
            w_biology: 1.5,
            w_carbon: 1.4,
            w_materials: 1.1,
        }
    }
}

/// Lyapunov residual V_t = Σ w_j * r_j^2.
#[derive(Clone, Copy, Debug)]
pub struct ResidualState {
    pub vt: RiskScalar,
}

impl ResidualState {
    pub fn from_risks(r: &RiskVector, w: &ResidualWeights) -> Self {
        w.assert_non_negative();
        let v =
            w.w_energy * r.energy.value().powi(2) +
            w.w_hydraulics * r.hydraulics.value().powi(2) +
            w.w_biology * r.biology.value().powi(2) +
            w.w_carbon * r.carbon.value().powi(2) +
            w.w_materials * r.materials.value().powi(2);
        ResidualState { vt: v }
    }

    /// Check Lyapunov invariant V_{t+1} ≤ V_t + ε.
    pub fn safestep_ok(&self, prev: &ResidualState, eps_vt: RiskScalar) -> bool {
        self.vt <= prev.vt + eps_vt
    }
}

/// Decision taken by ecosafety kernel for a proposed step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorridorDecision {
    Normal,
    Derate,
    Stop,
}

/// Rolling-window KER metrics.
#[derive(Clone, Copy, Debug)]
pub struct KerTriad {
    pub k_knowledge: RiskScalar,
    pub e_eco_impact: RiskScalar,
    pub r_risk_of_harm: RiskScalar,
}

#[derive(Clone, Debug)]
pub struct KerWindow {
    window_start: Instant,
    window_duration: Duration,
    steps_total: u64,
    steps_lyap_safe: u64,
    r_max: RiskScalar,
}

impl KerWindow {
    pub fn new(window_duration: Duration) -> Self {
        KerWindow {
            window_start: Instant::now(),
            window_duration,
            steps_total: 0,
            steps_lyap_safe: 0,
            r_max: 0.0,
        }
    }

    pub fn observe_step(&mut self, risk_vec: &RiskVector, lyap_safe: bool) {
        self.steps_total += 1;
        if lyap_safe {
            self.steps_lyap_safe += 1;
        }
        let r = risk_vec.max_coord().value();
        if r > self.r_max {
            self.r_max = r;
        }

        if self.window_start.elapsed() >= self.window_duration {
            // Reset window but keep last r as baseline.
            self.window_start = Instant::now();
            self.steps_total = 0;
            self.steps_lyap_safe = 0;
            self.r_max = r;
        }
    }

    pub fn triad(&self) -> KerTriad {
        let k = if self.steps_total == 0 {
            1.0
        } else {
            self.steps_lyap_safe as RiskScalar / self.steps_total as RiskScalar
        };
        let r = self.r_max.clamp(0.0, 1.0);
        let e = (1.0 - r).clamp(0.0, 1.0);
        KerTriad {
            k_knowledge: k,
            e_eco_impact: e,
            r_risk_of_harm: r,
        }
    }

    /// Production gate: K ≥ 0.90, E ≥ 0.90, R ≤ 0.13.
    pub fn production_admissible(&self) -> bool {
        let triad = self.triad();
        triad.k_knowledge >= 0.90 &&
        triad.e_eco_impact >= 0.90 &&
        triad.r_risk_of_harm <= 0.13
    }
}

/// Trait every Cyboquatic controller must implement: no action without a RiskVector.
pub trait SafeController<S, A> {
    /// Propose an actuation given current plant state.
    /// Must also emit a full RiskVector for ecosafety evaluation.
    fn propose_step(&mut self, state: &S) -> (A, RiskVector);
}

/// Ecosafety kernel that wraps controllers and enforces corridors and Lyapunov invariant.
pub struct EcoSafetyKernel<S, A> {
    pub residual_prev: ResidualState,
    pub weights: ResidualWeights,
    pub eps_vt: RiskScalar,
    pub window: KerWindow,
    _phantom_s: PhantomData<S>,
    _phantom_a: PhantomData<A>,
}

impl<S, A> EcoSafetyKernel<S, A> {
    pub fn new(weights: ResidualWeights, eps_vt: RiskScalar, window_duration: Duration) -> Self {
        Self {
            residual_prev: ResidualState { vt: 0.0 },
            weights,
            eps_vt,
            window: KerWindow::new(window_duration),
            _phantom_s: PhantomData,
            _phantom_a: PhantomData,
        }
    }

    /// Evaluate a proposed step and return (CorridorDecision, maybe_actuation).
    /// Actuation is None if the step is rejected or derated.
    pub fn evaluate_step<C>(
        &mut self,
        controller: &mut C,
        state: &S,
    ) -> (CorridorDecision, Option<A>)
    where
        C: SafeController<S, A>,
    {
        let (act, rv) = controller.propose_step(state);
        let residual_new = ResidualState::from_risks(&rv, &self.weights);

        let r_max = rv.max_coord().value();
        let hard_violation = (r_max - 1.0).abs() < 1e-9 || r_max > 1.0;

        let lyap_ok = residual_new.safestep_ok(&self.residual_prev, self.eps_vt);
        self.window.observe_step(&rv, lyap_ok && !hard_violation);

        self.residual_prev = residual_new;

        if hard_violation {
            return (CorridorDecision::Stop, None);
        }

        if !lyap_ok {
            // Soft breach: derate and do not actuate.
            return (CorridorDecision::Derate, None);
        }

        // Safe step: actuation allowed.
        (CorridorDecision::Normal, Some(act))
    }
}

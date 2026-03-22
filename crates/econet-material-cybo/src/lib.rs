//! econet-material-cybo
//! Biodegradable substrate traits for Cyboquatic machinery, bound into rx/Vt/KER.

#![forbid(unsafe_code)]

use cyboquatic_ecosafety_core::{CorridorBands, RiskCoord, RiskVector};

/// Kinetics and ecotoxicology metrics under Phoenix-class conditions.[file:11]
#[derive(Clone, Copy, Debug)]
pub struct MaterialKinetics {
    pub t90_days: f64,
    pub r_tox: f64,
    pub r_micro: f64,
    pub r_leach_cec: f64,
    pub r_pfas_resid: f64,
    pub caloric_density: f64,
}

/// Normalized material risks mapped into RiskCoord.[file:11]
#[derive(Clone, Copy, Debug)]
pub struct MaterialRisks {
    pub r_t90: RiskCoord,
    pub r_tox: RiskCoord,
    pub r_micro: RiskCoord,
    pub r_leach_cec: RiskCoord,
    pub r_pfas_resid: RiskCoord,
    pub r_caloric: RiskCoord,
}

impl MaterialRisks {
    pub fn from_kinetics(
        kin: &MaterialKinetics,
        t90_corr: CorridorBands,
        tox_corr: CorridorBands,
        micro_corr: CorridorBands,
        leach_corr: CorridorBands,
        pfas_corr: CorridorBands,
        caloric_corr: CorridorBands,
    ) -> Self {
        MaterialRisks {
            r_t90: t90_corr.normalize(kin.t90_days),
            r_tox: tox_corr.normalize(kin.r_tox),
            r_micro: micro_corr.normalize(kin.r_micro),
            r_leach_cec: leach_corr.normalize(kin.r_leach_cec),
            r_pfas_resid: pfas_corr.normalize(kin.r_pfas_resid),
            r_caloric: caloric_corr.normalize(kin.caloric_density),
        }
    }

    /// Aggregate into a single r_materials coordinate with tunable weights.[file:11][file:3]
    pub fn r_materials(
        &self,
        w_t90: f64,
        w_tox: f64,
        w_micro: f64,
        w_leach: f64,
        w_pfas: f64,
        w_caloric: f64,
    ) -> RiskCoord {
        let weights = [w_t90, w_tox, w_micro, w_leach, w_pfas, w_caloric];
        for w in &weights {
            assert!(*w >= 0.0);
        }
        let sum_w: f64 = weights.iter().sum();
        let norm = if sum_w <= 0.0 { 1.0 } else { sum_w };

        let v =
            w_t90 * self.r_t90.value() +
            w_tox * self.r_tox.value() +
            w_micro * self.r_micro.value() +
            w_leach * self.r_leach_cec.value() +
            w_pfas * self.r_pfas_resid.value() +
            w_caloric * self.r_caloric.value();

        RiskCoord::new(v / norm)
    }
}

/// Hard gate for biodegradable, non-toxic, non-baiting substrates.[file:11]
pub trait AntSafeSubstrate {
    fn corridor_ok(&self) -> bool;
}

/// Trait for compatibility with Cyboquatic node treatment goals.[file:11]
pub trait CyboNodeCompatible {
    fn compatible_with_node(&self, node_id: &str) -> bool;
}

/// Example substrate type implementing both traits.
#[derive(Clone, Debug)]
pub struct SubstrateStack {
    pub id: String,
    pub kinetics: MaterialKinetics,
    pub risks: MaterialRisks,
    pub ecoimpact_score: f64,
}

impl AntSafeSubstrate for SubstrateStack {
    fn corridor_ok(&self) -> bool {
        // Phoenix baseline corridors from your 2026 band.[file:11]
        let t90_hard_days = 180.0;
        let t90_gold_days = 120.0;
        let rtox_gold_max = 0.10;
        let rmicro_max = 0.05;
        let caloric_max = 0.30;

        let t90_ok = self.kinetics.t90_days <= t90_hard_days;
        let rtox_ok = self.kinetics.r_tox <= rtox_gold_max;
        let rmicro_ok = self.kinetics.r_micro <= rmicro_max;
        let caloric_ok = self.kinetics.caloric_density <= caloric_max;

        t90_ok && rtox_ok && rmicro_ok && caloric_ok
    }
}

impl CyboNodeCompatible for SubstrateStack {
    fn compatible_with_node(&self, _node_id: &str) -> bool {
        // Placeholder: in production this checks PFAS, nutrients, etc. against node corridors.[file:11]
        // Here we enforce at least that PFAS residue risk is kept in safe/gold bands.
        self.kinetics.r_pfas_resid <= 0.10
    }
}

/// Map material risks into the materials slot of a RiskVector.[file:3][file:11]
pub fn material_to_risk_vector(
    base: &RiskVector,
    mat_risks: &MaterialRisks,
    weights: (f64, f64, f64, f64, f64, f64),
) -> RiskVector {
    let (w_t90, w_tox, w_micro, w_leach, w_pfas, w_caloric) = weights;
    let r_mat = mat_risks.r_materials(
        w_t90, w_tox, w_micro, w_leach, w_pfas, w_caloric,
    );
    RiskVector {
        energy: base.energy,
        hydraulics: base.hydraulics,
        biology: base.biology,
        carbon: base.carbon,
        materials: r_mat,
    }
}

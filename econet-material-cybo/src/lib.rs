use serde::{Deserialize, Serialize};
use crate::risk::RiskCoord;

pub mod risk {
    pub use cyboquatic_ecosafety_core::RiskCoord;
}

/// Material properties measured under Phoenix-class conditions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaterialKinetics {
    pub t90_days: f64,      // time for 90% mass loss
    pub r_tox: f64,         // normalized toxicity 0–1
    pub r_micro: f64,       // micro-residue risk 0–1
    pub r_leach_cec: f64,   // CEC leachate risk 0–1
    pub r_pfas_resid: f64,  // PFAS residue risk 0–1
    pub caloric_density: f64, // kJ/g to prevent baiting
}

/// Hard-coded Phoenix corridors; in production loaded from qpudatashards.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaterialCorridors {
    pub t90_max_days_hard: f64,
    pub t90_gold_days: f64,
    pub r_tox_gold_max: f64,
    pub r_micro_max: f64,
    pub r_leach_cec_max: f64,
    pub r_pfas_resid_max: f64,
    pub caloric_density_max: f64,
}

impl Default for MaterialCorridors {
    fn default() -> Self {
        MaterialCorridors {
            t90_max_days_hard: 180.0,
            t90_gold_days: 120.0,
            r_tox_gold_max: 0.10,
            r_micro_max: 0.05,
            r_leach_cec_max: 0.10,
            r_pfas_resid_max: 0.10,
            caloric_density_max: 0.30,
        }
    }
}

/// Normalized material risk coordinates.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaterialRisks {
    pub r_t90: RiskCoord,
    pub r_tox: RiskCoord,
    pub r_micro: RiskCoord,
    pub r_leach_cec: RiskCoord,
    pub r_pfas_resid: RiskCoord,
}

impl MaterialRisks {
    pub fn from_kinetics(k: &MaterialKinetics, c: &MaterialCorridors) -> Self {
        let r_t90 = if k.t90_days <= c.t90_gold_days {
            RiskCoord(0.0)
        } else if k.t90_days >= c.t90_max_days_hard {
            RiskCoord(1.0)
        } else {
            let t = (k.t90_days - c.t90_gold_days)
                / (c.t90_max_days_hard - c.t90_gold_days).max(1e-12);
            RiskCoord::clamped(0.5 * t)
        };
        let r_tox = RiskCoord::clamped(k.r_tox / c.r_tox_gold_max.max(1e-12));
        let r_micro = RiskCoord::clamped(k.r_micro / c.r_micro_max.max(1e-12));
        let r_leach_cec = RiskCoord::clamped(k.r_leach_cec / c.r_leach_cec_max.max(1e-12));
        let r_pfas_resid = RiskCoord::clamped(k.r_pfas_resid / c.r_pfas_resid_max.max(1e-12));

        MaterialRisks {
            r_t90,
            r_tox,
            r_micro,
            r_leach_cec,
            r_pfas_resid,
        }
    }

    /// Composite materials plane risk r_materials.
    pub fn r_materials(&self, w_t90: f64, w_tox: f64, w_micro: f64,
                       w_leach: f64, w_pfas: f64) -> RiskCoord {
        let num = w_t90 * self.r_t90.0
            + w_tox * self.r_tox.0
            + w_micro * self.r_micro.0
            + w_leach * self.r_leach_cec.0
            + w_pfas * self.r_pfas_resid.0;
        let den = (w_t90 + w_tox + w_micro + w_leach + w_pfas).max(1e-12);
        RiskCoord::clamped(num / den)
    }
}

/// Trait every deployable Cyboquatic substrate must satisfy.
pub trait AntSafeSubstrate {
    fn kinetics(&self) -> &MaterialKinetics;
    fn corridors(&self) -> &MaterialCorridors;

    fn risks(&self) -> MaterialRisks {
        MaterialRisks::from_kinetics(self.kinetics(), self.corridors())
    }

    /// Hard gate: all corridors must hold; otherwise this material is non-deployable.
    fn corridor_ok(&self) -> bool {
        let k = self.kinetics();
        let c = self.corridors();
        if k.t90_days > c.t90_max_days_hard {
            return false;
        }
        if k.r_tox > c.r_tox_gold_max {
            return false;
        }
        if k.r_micro > c.r_micro_max {
            return false;
        }
        if k.r_leach_cec > c.r_leach_cec_max {
            return false;
        }
        if k.r_pfas_resid > c.r_pfas_resid_max {
            return false;
        }
        if k.caloric_density > c.caloric_density_max {
            return false;
        }
        true
    }
}

/// Trait ensuring substrate does not conflict with node treatment goals.
pub trait CyboNodeCompatible {
    fn introduces_pfas(&self) -> bool;
    fn introduces_nutrients(&self) -> bool;
    fn introduces_pathogens(&self) -> bool;

    fn node_compatible(&self) -> bool {
        !(self.introduces_pfas()
            || self.introduces_nutrients()
            || self.introduces_pathogens())
    }
}

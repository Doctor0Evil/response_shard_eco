// File: crates/cyboquatic-materials/src/lib.rs

use cyboquatic_ecosafety_core::{RiskCoord, RiskVector};

/// Material-level risk coordinates derived from C++ decomposition_sim.
#[derive(Clone, Copy, Debug)]
pub struct MaterialRisks {
    pub r_t90: RiskCoord,
    pub r_tox: RiskCoord,
    pub r_micro: RiskCoord,
}

impl MaterialRisks {
    pub fn composite_materials(&self) -> RiskCoord {
        // Example weights: emphasize toxicity and micro-residue.
        let w_t90 = 0.3;
        let w_tox = 0.4;
        let w_micro = 0.3;

        let v = w_t90 * self.r_t90.value
              + w_tox * self.r_tox.value
              + w_micro * self.r_micro.value;
        RiskCoord::clamped(v)
    }
}

/// Trait gate: only substrates within corridors may be instantiated.
pub trait SafeSubstrate {
    fn material_risks(&self) -> MaterialRisks;

    fn r_materials(&self) -> RiskCoord {
        self.material_risks().composite_materials()
    }

    fn corridor_ok(&self, hard_max: f64) -> bool {
        self.r_materials().value <= hard_max
    }
}

/// Example substrate spec (to be hydrated from ALN shard).
pub struct SubstrateSpec {
    pub name: String,
    pub risks: MaterialRisks,
}

impl SafeSubstrate for SubstrateSpec {
    fn material_risks(&self) -> MaterialRisks {
        self.risks
    }
}

/// Lift material risk into machine-wide RiskVector.
pub fn with_material_plane(mut rv: RiskVector, r_materials: RiskCoord) -> RiskVector {
    rv.r_materials = r_materials;
    rv
}

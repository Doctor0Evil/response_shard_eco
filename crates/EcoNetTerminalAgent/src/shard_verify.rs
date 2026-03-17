// Lightweight, local qpudatashard verification.

#![forbid(unsafe_code)]

use crate::resource_corridors::{CorridorDecision, Residual, safe_step};

#[derive(Clone, Debug)]
pub struct ShardHeader {
    pub shard_type: String,
    pub region: String,
    pub timestamputc: String,
    pub did_author: String,
    pub did_signature_hex: String,
}

#[derive(Clone, Debug)]
pub struct TerminalCorridorRow {
    pub var_id: String,
    pub measured_frac: f64, // e.g. CPU fraction 0–1
    pub safe: f64,
    pub gold: f64,
    pub hard: f64,
    pub weight: f64,
    pub lyap_channel: u8,
}

#[derive(Clone, Debug)]
pub struct EcoNetTerminalCorridorShard {
    pub header: ShardHeader,
    pub rows: Vec<TerminalCorridorRow>,
    pub knowledge_factor: f64,
    pub eco_impact_value: f64,
    pub risk_of_harm: f64,
}

impl EcoNetTerminalCorridorShard {
    pub fn to_residual(&self) -> Option<Residual> {
        let mut coords = Vec::with_capacity(self.rows.len());
        for r in &self.rows {
            if r.measured_frac < 0.0 || r.measured_frac > 1.0 {
                return None;
            }
            coords.push(crate::resource_corridors::RiskCoord {
                value: r.measured_frac,
                bands: crate::resource_corridors::CorridorBands {
                    var_id: Box::leak(r.var_id.clone().into_boxed_str()),
                    safe: r.safe,
                    gold: r.gold,
                    hard: r.hard,
                    weight: r.weight,
                    lyap_channel: r.lyap_channel,
                },
            });
        }
        let mut res = Residual { vt: 0.0, coords };
        res.recompute();
        Some(res)
    }
}

pub fn verify_corridor_chain(
    shards: &[EcoNetTerminalCorridorShard],
) -> CorridorDecision {
    if shards.len() < 2 {
        return CorridorDecision::Ok;
    }
    let mut decision = CorridorDecision::Ok;
    for w in shards.windows(2) {
        let prev_res = match w[0].to_residual() {
            Some(r) => r,
            None => return CorridorDecision::Stop,
        };
        let next_res = match w[1].to_residual() {
            Some(r) => r,
            None => return CorridorDecision::Stop,
        };
        let step = safe_step(&prev_res, &next_res);
        match step {
            CorridorDecision::Stop => return CorridorDecision::Stop,
            CorridorDecision::Derate => decision = CorridorDecision::Derate,
            CorridorDecision::Ok => {}
        }
    }
    decision
}

pub struct RiskCoord {
    pub name: String,      // e.g. "r_PFAS"
    pub value: f64,        // 0–1 normalized
    pub safe: f64,         // upper bound of green band
    pub gold: f64,         // aspirational target
    pub hard: f64,         // hard edge at 1.0
    pub w: f64,            // weight in V_t
    pub lyap_channel: String,
    pub mandatory: bool,
}

pub struct RiskVector {
    pub id: String,            // e.g. "NODE:MARVAULT01"
    pub coords: Vec<RiskCoord> // must include all mandatory rx for node
}

pub struct LyapunovResidual {
    pub system_id: String,     // node or cluster id
    pub t: f64,
    pub value: f64,            // V_t
    pub d_value_dt: f64,
    pub stable: bool,          // invariant V_{t+1} <= V_t holds?
}

pub enum NodeAction {
    Normal,
    Derate(f64),
    Stop,
}

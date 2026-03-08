use crate::{RiskVector, LyapunovResidual, NodeAction};

/// Fail if any mandatory rx missing or malformed.
pub fn corridor_present(rv: &RiskVector, mandatory: &[&str]) -> Result<(), String> {
    for &name in mandatory {
        let found = rv.coords.iter().find(|c| c.name == name);
        match found {
            None => return Err(format!("missing mandatory rx: {}", name)),
            Some(c) => {
                if !(0.0 <= c.safe && c.safe <= c.gold && c.gold <= c.hard && c.hard <= 1.0) {
                    return Err(format!("malformed bands for {}", name));
                }
            }
        }
    }
    Ok(())
}

/// Normalize raw values → 0–1; here we assume already normalized.
pub fn normalize_rx(rv: &RiskVector) -> RiskVector {
    rv.clone()
}

/// Compute V_t as weighted sum of (value - safe)+.
pub fn compute_Vt(rv: &RiskVector) -> LyapunovResidual {
    let mut v = 0.0;
    for c in &rv.coords {
        if c.value > c.safe {
            v += c.w * (c.value - c.safe);
        }
    }
    LyapunovResidual {
        system_id: rv.id.clone(),
        t: 0.0,
        value: v,
        d_value_dt: 0.0,
        stable: true, // caller fills with history; spine checks monotonicity
    }
}

/// Enforce safe_step: hard-edge or V_{t+1} increase ⇒ Stop/Derate.
pub fn safe_step(
    rv_next: &RiskVector,
    vt_prev: &LyapunovResidual,
    vt_next: &LyapunovResidual,
) -> NodeAction {
    // Any hard-edge breach?
    let hard_breach = rv_next
        .coords
        .iter()
        .any(|c| c.value >= c.hard || c.value >= 1.0);
    if hard_breach {
        return NodeAction::Stop;
    }

    // V_t growth outside interior?
    if vt_next.value > vt_prev.value && vt_prev.value > 0.0 {
        return NodeAction::Derate(0.5);
    }

    NodeAction::Normal
}

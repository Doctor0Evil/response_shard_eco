// filename: crates/cyboquatic-ecosafety-core/src/controller.rs

use crate::core::{RiskVector, Residual, LyapunovWeights, SafeDecision, safestep, SafeStepConfig};

pub trait SafeController {
    type State;
    type Actuation;

    fn propose_step(
        &mut self,
        state: &Self::State,
        prev_resid: Residual,
        w: &LyapunovWeights,
    ) -> (Self::Actuation, RiskVector, Residual);
}

pub fn route_and_actuate<C>(
    ctrl: &mut C,
    state: &C::State,
    prev_resid: Residual,
    w: &LyapunovWeights,
    cfg: &SafeStepConfig,
    apply: &mut dyn FnMut(&C::Actuation),
) -> (SafeDecision, Residual)
where
    C: SafeController,
{
    let (act, rv_next, resid_next) = ctrl.propose_step(state, prev_resid, w);
    let decision = safestep(prev_resid, resid_next, &rv_next, cfg);

    match decision {
        SafeDecision::Accept => {
            // Only actuate when carbon AND biodiversity are inside corridors
            if rv_next.r_carbon < 1.0 && rv_next.r_biodiversity < 1.0 {
                apply(&act);
            }
        }
        SafeDecision::Derate | SafeDecision::Stop => {
            // No actuation, higher layers can re-plan
        }
    }

    (decision, resid_next)
}

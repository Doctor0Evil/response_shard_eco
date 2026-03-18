// File: opt-cyboquatic-fog-router/src/main.rs

#![forbid(unsafe_code)]

use std::time::Instant;
use cyboquatic_ecosafety_core::{KerTriad, RiskCoord, RiskVector};

#[derive(Clone, Copy, Debug)]
pub enum MediaClass {
    WaterOnly,
    WaterBiofilm,
    AirPlenum,
}

#[derive(Clone, Copy, Debug)]
pub struct CyboVariant {
    pub id: u64,
    pub energy_req_j: f64,
    pub safety_factor: f64,
    pub max_latency_ms: u64,
    pub media: MediaClass,
    pub hydraulic_impact: f64,
    pub dvt_nominal: f64,
}

#[derive(Clone, Copy, Debug)]
pub enum BioSurfaceMode {
    Raw,
    Preprocessed,
    Restricted,
}

#[derive(Clone, Copy, Debug)]
pub struct NodeShard {
    pub esurplus_j: f64,
    pub pmargin_kw: f64,
    pub tailwind_mode: bool,
    pub d_edt_w: f64,
    pub q_m3s: f64,
    pub hlr_m_per_h: f64,
    pub surcharge_risk_rx: f64,
    pub r_pathogen: f64,
    pub r_fouling: f64,
    pub r_cec: f64,
    pub biosurface_mode: BioSurfaceMode,
    pub vt_local: f64,
    pub vt_trend: f64,
    pub kscore: f64,
    pub escore: f64,
    pub rscore: f64,
}

#[derive(Clone, Copy, Debug)]
pub enum RouteDecision {
    Accept,
    Reject,
    Reroute,
}

#[derive(Clone, Copy, Debug)]
pub struct RoutingContext {
    pub vt_global: f64,
    pub vt_global_next_max: f64,
    pub now: Instant,
}

fn tailwind_valid(node: &NodeShard, variant: &CyboVariant) -> bool {
    if !node.tailwind_mode {
        return false;
    }
    let required = variant.energy_req_j * variant.safety_factor.max(1.0);
    node.esurplus_j > required
        && node.pmargin_kw > 0.0
        && node.d_edt_w >= 0.0
}

fn biosurface_ok(node: &NodeShard, variant: &CyboVariant) -> bool {
    match node.biosurface_mode {
        BioSurfaceMode::Restricted => matches!(variant.media, MediaClass::AirPlenum),
        BioSurfaceMode::Raw | BioSurfaceMode::Preprocessed => {
            let rthresh = 0.5_f64;
            match variant.media {
                MediaClass::AirPlenum => node.r_pathogen < rthresh,
                MediaClass::WaterOnly | MediaClass::WaterBiofilm => {
                    matches!(node.biosurface_mode, BioSurfaceMode::Preprocessed)
                        && node.r_pathogen < rthresh
                        && node.r_fouling < rthresh
                        && node.r_cec < rthresh
                }
            }
        }
    }
}

fn hydraulic_ok(node: &NodeShard, variant: &CyboVariant) -> bool {
    let impact = variant.hydraulic_impact.max(0.0);
    let rx = node.surcharge_risk_rx.max(0.0);
    let predicted = rx + impact;
    predicted < 1.0
}

fn lyapunov_ok(node: &NodeShard, variant: &CyboVariant, ctx: &RoutingContext) -> bool {
    let dv_local = variant.dvt_nominal;
    let vt_next_est = ctx.vt_global + dv_local;
    vt_next_est <= ctx.vt_global_next_max && dv_local + node.vt_trend <= 0.0
}

pub fn route_variant(
    variant: &CyboVariant,
    node: &NodeShard,
    ctx: &RoutingContext,
) -> RouteDecision {
    if !tailwind_valid(node, variant) {
        return RouteDecision::Reroute;
    }
    if !biosurface_ok(node, variant) {
        return RouteDecision::Reroute;
    }
    if !hydraulic_ok(node, variant) {
        return RouteDecision::Reroute;
    }
    if !lyapunov_ok(node, variant, ctx) {
        return RouteDecision::Reject;
    }
    RouteDecision::Accept
}

fn main() {
    let ctx = RoutingContext {
        vt_global: 1.0,
        vt_global_next_max: 1.0,
        now: Instant::now(),
    };

    let node = NodeShard {
        esurplus_j: 5_000.0,
        pmargin_kw: 3.5,
        tailwind_mode: true,
        d_edt_w: 10.0,
        q_m3s: 0.2,
        hlr_m_per_h: 5.0,
        surcharge_risk_rx: 0.2,
        r_pathogen: 0.1,
        r_fouling: 0.3,
        r_cec: 0.2,
        biosurface_mode: BioSurfaceMode::Preprocessed,
        vt_local: 0.9,
        vt_trend: -0.01,
        kscore: 0.93,
        escore: 0.90,
        rscore: 0.14,
    };

    let variant = CyboVariant {
        id: 42,
        energy_req_j: 500.0,
        safety_factor: 1.5,
        max_latency_ms: 200,
        media: MediaClass::WaterOnly,
        hydraulic_impact: 0.1,
        dvt_nominal: -0.001,
    };

    let decision = route_variant(&variant, &node, &ctx);
    println!("Routing decision for {}: {:?}", variant.id, decision);
}

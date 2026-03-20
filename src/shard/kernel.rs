//! # ResponseShard Kernel Module
//! 
//! Core data structures and computation logic for KER triads (Knowledge, Eco-impact, Risk)
//! and ResponseShard production within the Cyboquatics ecological restoration framework.
//! 
//! This module implements the frozen formulas for:
//! - K = N_corridor-backed / N_critical
//! - E = mass/volume kernel B computations
//! - R = weighted corridor penetration with non-increasing residual V_t
//! 
//! All structures are designed for direct qpudatashard serialization and ALN contract integration.

#![deny(clippy::all)]
#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during KER computation or shard validation
#[derive(Error, Debug, Clone, PartialEq)]
pub enum KernelError {
    #[error("Corridor band violation: {variable} exceeded limit {limit} with value {actual}")]
    CorridorViolation {
        variable: String,
        limit: f64,
        actual: f64,
    },
    
    #[error("Residual risk increase detected: V_t+1 ({next}) > V_t ({current})")]
    ResidualRiskIncrease {
        current: f64,
        next: f64,
    },
    
    #[error("Invalid KER component: {component} must be in range [0, 1], got {value}")]
    InvalidKerComponent {
        component: String,
        value: f64,
    },
    
    #[error("Missing required corridor definition for critical variable: {variable}")]
    MissingCorridor {
        variable: String,
    },
    
    #[error("Bostrom DID validation failed: {reason}")]
    DidValidationFailed {
        reason: String,
    },
}

// ============================================================================
// Core Data Structures
// ============================================================================

/// Bostrom Decentralized Identifier for persistent entity tracking
/// 
/// Anchors every ResponseShard to a specific entity (agent or human contributor)
/// ensuring long-term identity, accountability, and reward continuity over 20-50 year horizons.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BostromDid {
    /// The DID method specifier (e.g., "did:bostrom:ecotribute")
    pub method: String,
    /// The unique identifier string (base58-encoded public key or hash)
    pub identifier: String,
    /// Version number for identity evolution tracking
    pub version: u32,
    /// Unix timestamp of identity creation
    pub created_at: u64,
    /// Hex-stamped ALN contract version this identity is bound to
    pub contract_version: String,
}

impl BostromDid {
    /// Creates a new BostromDid with validation
    pub fn new(
        identifier: String,
        version: u32,
        contract_version: String,
    ) -> Result<Self, KernelError> {
        if identifier.is_empty() {
            return Err(KernelError::DidValidationFailed {
                reason: "Identifier cannot be empty".to_string(),
            });
        }
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| KernelError::DidValidationFailed {
                reason: "System time error".to_string(),
            })?
            .as_secs();
        
        Ok(Self {
            method: "did:bostrom:ecotribute".to_string(),
            identifier,
            version,
            created_at: now,
            contract_version,
        })
    }
    
    /// Returns the full DID string representation
    pub fn to_did_string(&self) -> String {
        format!(
            "{}:{}#v{}",
            self.method, self.identifier, self.version
        )
    }
    
    /// Validates the DID against ALN registry
    pub fn validate(&self, registry_hash: &str) -> Result<(), KernelError> {
        // In production, this would query the ALN identity registry
        if registry_hash.is_empty() {
            return Err(KernelError::DidValidationFailed {
                reason: "Empty registry hash".to_string(),
            });
        }
        Ok(())
    }
}

/// Knowledge-Eco-Risk Triad Components
/// 
/// Each component is normalized to [0, 1] range for consistent computation
/// and cross-domain comparison across MAR cells, terminals, and Cyboquatic nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KerTriad {
    /// Knowledge-factor: K = N_corridor-backed / N_critical
    /// Measures proportion of knowledge grounded in established safety corridors
    pub knowledge: f64,
    
    /// Eco-impact: Computed from mass/volume kernel B
    /// Quantifies positive environmental outcome of an action
    pub eco_impact: f64,
    
    /// Risk-of-harm: Weighted corridor penetration score
    /// Directly quantifies deviation from established safe operating limits
    pub risk: f64,
}

impl KerTriad {
    /// Creates a new KER triad with validation
    pub fn new(knowledge: f64, eco_impact: f64, risk: f64) -> Result<Self, KernelError> {
        // Validate all components are in [0, 1] range
        for (component, value) in [
            ("knowledge", knowledge),
            ("eco_impact", eco_impact),
            ("risk", risk),
        ] {
            if value < 0.0 || value > 1.0 {
                return Err(KernelError::InvalidKerComponent {
                    component: component.to_string(),
                    value,
                });
            }
        }
        
        Ok(Self {
            knowledge,
            eco_impact,
            risk,
        })
    }
    
    /// Computes the composite safety score (higher is better)
    pub fn safety_score(&self) -> f64 {
        // Weighted combination favoring low risk and high eco-impact
        (self.eco_impact * 0.5) + ((1.0 - self.risk) * 0.5)
    }
    
    /// Checks if this triad meets minimum thresholds for step advancement
    pub fn meets_step_threshold(&self, min_k: f64, min_e: f64, max_r: f64) -> bool {
        self.knowledge >= min_k 
            && self.eco_impact >= min_e 
            && self.risk <= max_r
    }
}

/// Risk Coordinate for a single critical variable
/// 
/// Maps all critical variables into risk coordinates r_j ∈ [0, 1]
/// Used for aggregate residual risk computation V_t = Σ w_j * r_j²
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskCoord {
    /// Variable name (e.g., "toxicity", "microplastics", "cpu_usage")
    pub variable: String,
    /// Normalized risk value in [0, 1]
    pub risk_value: f64,
    /// Weight for aggregate computation
    pub weight: f64,
    /// Corridor band this coordinate is measured against
    pub corridor_band: CorridorBand,
}

impl RiskCoord {
    pub fn new(
        variable: String,
        risk_value: f64,
        weight: f64,
        corridor_band: CorridorBand,
    ) -> Result<Self, KernelError> {
        if risk_value < 0.0 || risk_value > 1.0 {
            return Err(KernelError::InvalidKerComponent {
                component: format!("risk_value for {}", variable),
                value: risk_value,
            });
        }
        
        Ok(Self {
            variable,
            risk_value,
            weight,
            corridor_band,
        })
    }
}

/// Corridor Band Definition
/// 
/// Defines safety boundaries for critical variables with mandatory
/// toxicity, HLR, microplastics, CPU/RAM, and other ecosystem constraints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorridorBand {
    /// Variable name this corridor governs
    pub variable: String,
    /// Minimum acceptable value
    pub min: f64,
    /// Maximum acceptable value
    pub max: f64,
    /// Warning threshold (trigger before violation)
    pub warning_threshold: f64,
    /// Version of this corridor definition
    pub version: String,
}

impl CorridorBand {
    /// Checks if a value violates this corridor
    pub fn violates(&self, value: f64) -> bool {
        value < self.min || value > self.max
    }
    
    /// Checks if a value is in warning zone
    pub fn in_warning_zone(&self, value: f64) -> bool {
        let distance_to_max = self.max - value;
        let distance_to_min = value - self.min;
        let range = self.max - self.min;
        
        distance_to_max < (range * self.warning_threshold) 
            || distance_to_min < (range * self.warning_threshold)
    }
}

/// Residual Risk State
/// 
/// Aggregates all risk coordinates into single residual score V_t
/// Enforces non-increasing constraint: V_t+1 ≤ V_t
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Residual {
    /// Current residual risk score V_t
    pub current: f64,
    /// Previous residual risk score V_t-1
    pub previous: f64,
    /// Timestamp of current measurement
    pub timestamp: u64,
    /// Risk coordinates contributing to this residual
    pub coordinates: Vec<RiskCoord>,
}

impl Residual {
    /// Computes residual from risk coordinates: V_t = Σ w_j * r_j²
    pub fn from_coordinates(coordinates: Vec<RiskCoord>) -> Result<Self, KernelError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| KernelError::ResidualRiskIncrease {
                current: 0.0,
                next: 0.0,
            })?
            .as_secs();
        
        let current: f64 = coordinates
            .iter()
            .map(|coord| coord.weight * coord.risk_value.powi(2))
            .sum();
        
        Ok(Self {
            current,
            previous: current, // Initialized equal; updated on next measurement
            timestamp: now,
            coordinates,
        })
    }
    
    /// Validates non-increasing constraint: V_t+1 ≤ V_t
    pub fn validate_non_increasing(&self, next_value: f64) -> Result<(), KernelError> {
        if next_value > self.current {
            return Err(KernelError::ResidualRiskIncrease {
                current: self.current,
                next: next_value,
            });
        }
        Ok(())
    }
    
    /// Updates residual with new measurement
    pub fn update(&mut self, new_coordinates: Vec<RiskCoord>) -> Result<(), KernelError> {
        let new_residual = Self::from_coordinates(new_coordinates)?;
        self.validate_non_increasing(new_residual.current)?;
        
        self.previous = self.current;
        self.current = new_residual.current;
        self.timestamp = new_residual.timestamp;
        self.coordinates = new_residual.coordinates;
        
        Ok(())
    }
}

/// Time Window for Progressive Timing
/// 
/// Defines explicit windows w for eco-wealth and burns using
/// CEIM-derived EcoImpactScore and KER per window (5-15 min, daily, quarterly)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeWindow {
    /// Window identifier
    pub id: String,
    /// Window type (short, daily, quarterly)
    pub window_type: WindowType,
    /// Start timestamp
    pub start: u64,
    /// End timestamp
    pub end: u64,
    /// KER triad computed for this window
    pub ker: KerTriad,
    /// Residual risk at window close
    pub residual: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WindowType {
    Short,    // 5-15 minutes
    Daily,    // 24 hours
    Quarterly, // 3 months
}

/// ResponseShard - The fundamental unit of automated knowledge production
/// 
/// Every research or ops action produces a ResponseShard with:
/// - Bostrom DID for identity
/// - Topic tag for classification
/// - KER triad for quantification
/// - At least one tightened corridor or new equation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseShard {
    /// Unique shard identifier
    pub shard_id: String,
    /// Producer's Bostrom DID
    pub producer_did: BostromDid,
    /// Topic classification tag
    pub topic_tag: String,
    /// KER triad for this shard
    pub ker: KerTriad,
    /// Corridor updates included in this shard
    pub corridor_updates: Vec<CorridorBand>,
    /// Equations or knowledge contributions
    pub equations: Vec<String>,
    /// Time window this shard belongs to
    pub time_window: TimeWindow,
    /// Creation timestamp
    pub created_at: u64,
    /// Hex-stamp for ALN contract versioning
    pub contract_hex_stamp: String,
    /// EcoImpactScore from CEIM mass kernel
    pub eco_impact_score: f64,
    /// Node ID for cross-node coordination
    pub node_id: String,
}

impl ResponseShard {
    /// Creates a new ResponseShard with full validation
    pub fn new(
        shard_id: String,
        producer_did: BostromDid,
        topic_tag: String,
        ker: KerTriad,
        corridor_updates: Vec<CorridorBand>,
        time_window: TimeWindow,
        node_id: String,
        contract_hex_stamp: String,
    ) -> Result<Self, KernelError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| KernelError::DidValidationFailed {
                reason: "System time error".to_string(),
            })?
            .as_secs();
        
        // Validate at least one corridor update or equation
        if corridor_updates.is_empty() {
            return Err(KernelError::MissingCorridor {
                variable: "At least one corridor update required".to_string(),
            });
        }
        
        Ok(Self {
            shard_id,
            producer_did,
            topic_tag,
            ker,
            corridor_updates,
            equations: Vec::new(),
            time_window,
            created_at: now,
            contract_hex_stamp,
            eco_impact_score: ker.eco_impact,
            node_id,
        })
    }
    
    /// Adds an equation to this shard
    pub fn add_equation(&mut self, equation: String) {
        self.equations.push(equation);
    }
    
    /// Validates shard against all corridor constraints
    pub fn validate_corridors(&self) -> Result<(), KernelError> {
        for corridor in &self.corridor_updates {
            // In production, validate against actual measured values
            if corridor.min >= corridor.max {
                return Err(KernelError::CorridorViolation {
                    variable: corridor.variable.clone(),
                    limit: corridor.max,
                    actual: corridor.min,
                });
            }
        }
        Ok(())
    }
    
    /// Serializes shard to qpudatashard CSV format
    pub fn to_csv_row(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{},{}",
            self.shard_id,
            self.producer_did.to_did_string(),
            self.topic_tag,
            self.ker.knowledge,
            self.ker.eco_impact,
            self.ker.risk,
            self.eco_impact_score,
            self.node_id,
            self.time_window.id,
            self.created_at
        )
    }
}

// ============================================================================
// KER Computation Engine
// ============================================================================

/// Computes Knowledge-factor: K = N_corridor-backed / N_critical
pub fn compute_knowledge_factor(
    corridor_backed_count: u32,
    critical_count: u32,
) -> Result<f64, KernelError> {
    if critical_count == 0 {
        return Err(KernelError::InvalidKerComponent {
            component: "knowledge".to_string(),
            value: f64::INFINITY,
        });
    }
    
    let k = corridor_backed_count as f64 / critical_count as f64;
    
    if k > 1.0 {
        return Err(KernelError::InvalidKerComponent {
            component: "knowledge".to_string(),
            value: k,
        });
    }
    
    Ok(k)
}

/// Computes Eco-impact from mass/volume kernel B
pub fn compute_eco_impact(
    mass_removed_kg: f64,
    max_capacity_kg: f64,
    efficiency_factor: f64,
) -> Result<f64, KernelError> {
    if max_capacity_kg <= 0.0 {
        return Err(KernelError::InvalidKerComponent {
            component: "eco_impact".to_string(),
            value: f64::INFINITY,
        });
    }
    
    let e = (mass_removed_kg / max_capacity_kg) * efficiency_factor;
    let e = e.min(1.0); // Cap at 1.0
    
    if e < 0.0 {
        return Err(KernelError::InvalidKerComponent {
            component: "eco_impact".to_string(),
            value: e,
        });
    }
    
    Ok(e)
}

/// Computes Risk-of-harm as weighted corridor penetration
pub fn compute_risk_score(
    risk_coords: &[RiskCoord],
) -> Result<f64, KernelError> {
    let total_weight: f64 = risk_coords.iter().map(|c| c.weight).sum();
    
    if total_weight == 0.0 {
        return Ok(0.0);
    }
    
    let risk: f64 = risk_coords
        .iter()
        .map(|coord| coord.weight * coord.risk_value)
        .sum::<f64>() / total_weight;
    
    if risk < 0.0 || risk > 1.0 {
        return Err(KernelError::InvalidKerComponent {
            component: "risk".to_string(),
            value: risk,
        });
    }
    
    Ok(risk)
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ker_triad_creation_valid() {
        let ker = KerTriad::new(0.94, 0.90, 0.13).unwrap();
        assert_eq!(ker.knowledge, 0.94);
        assert_eq!(ker.eco_impact, 0.90);
        assert_eq!(ker.risk, 0.13);
    }
    
    #[test]
    fn test_ker_triad_creation_invalid() {
        let result = KerTriad::new(1.5, 0.90, 0.13);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_residual_non_increasing() {
        let coords = vec![
            RiskCoord::new(
                "toxicity".to_string(),
                0.3,
                0.5,
                CorridorBand {
                    variable: "toxicity".to_string(),
                    min: 0.0,
                    max: 1.0,
                    warning_threshold: 0.1,
                    version: "v1.0".to_string(),
                },
            ).unwrap(),
        ];
        
        let mut residual = Residual::from_coordinates(coords.clone()).unwrap();
        let result = residual.validate_non_increasing(0.25);
        assert!(result.is_ok());
        
        let result = residual.validate_non_increasing(0.35);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_response_shard_csv_serialization() {
        let did = BostromDid::new(
            "abc123".to_string(),
            1,
            "0x7f8a9b".to_string(),
        ).unwrap();
        
        let ker = KerTriad::new(0.94, 0.90, 0.13).unwrap();
        
        let corridor = CorridorBand {
            variable: "microplastics".to_string(),
            min: 0.0,
            max: 0.5,
            warning_threshold: 0.1,
            version: "v1.0".to_string(),
        };
        
        let window = TimeWindow {
            id: "w_20260320_001".to_string(),
            window_type: WindowType::Daily,
            start: 1774425600,
            end: 1774512000,
            ker: ker.clone(),
            residual: 0.13,
        };
        
        let shard = ResponseShard::new(
            "shard_001".to_string(),
            did,
            "biodegradation".to_string(),
            ker,
            vec![corridor],
            window,
            "node_phoenix_01".to_string(),
            "0x7f8a9b".to_string(),
        ).unwrap();
        
        let csv = shard.to_csv_row();
        assert!(csv.contains("shard_001"));
        assert!(csv.contains("did:bostrom:ecotribute"));
    }
}

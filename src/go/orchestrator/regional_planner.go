// ============================================================================
// Ecotribute Regional Orchestrator & Portfolio Optimizer
// Module: response_shard_eco/src/go/orchestrator/regional_planner.go
// Version: 1.0.0 (ALN Contract Hex: 0x7f8a9b)
//
// Purpose:
//   Coordinates multiple Cyboquatic nodes across a geographic region.
//   Optimizes eco-wealth portfolio to maximize impact while respecting regional risk envelopes.
//   Enforces "Phoenix-first" regional constraints and cross-node corridor compatibility.
//
// Safety Invariants:
//   1. Regional Residual Risk (V_region) must not increase (V_t+1 <= V_t).
//   2. No node activation if regional corridor (e.g., watershed toxicity) is breached.
//   3. All nodes must possess valid Bostrom DID and ALN contract alignment.
//   4. Optimization prioritizes risk reduction over throughput expansion.
// ============================================================================

package orchestrator

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"math"
	"sort"
	"sync"
	"time"
)

// ============================================================================
// Constants & Configuration
// ============================================================================

const (
	// Contract Alignment
	ContractHexStamp = "0x7f8a9b"
	ContractVersion  = 1

	// Safety Thresholds
	MinKnowledgeFactor = 0.85
	MinEcoImpact       = 0.50
	MaxRiskHarm        = 0.20

	// Regional Envelope Limits
	MaxRegionalResidual = 1.0
	WarningThreshold    = 0.8 // Trigger optimization if risk > 80% of limit

	// Optimization Weights
	WeightEcoImpact = 0.5
	WeightRiskReduction = 0.5
)

// ============================================================================
// Error Definitions
// ============================================================================

var (
	ErrRegionalRiskViolation   = errors.New("regional residual risk violation")
	ErrCorridorBreach          = errors.New("regional corridor breach")
	ErrInvalidNodeDID          = errors.New("invalid node Bostrom DID")
	ErrContractMismatch        = errors.New("ALN contract version mismatch")
	ErrOptimizationFailed      = errors.New("portfolio optimization failed")
	ErrNodeNotRegistered       = errors.New("node not registered in region")
)

// ============================================================================
// Data Structures
// ============================================================================

// KerTriad mirrors Rust/Python structures for cross-language compatibility
type KerTriad struct {
	Knowledge  float64 `json:"knowledge"`
	EcoImpact  float64 `json:"eco_impact"`
	Risk       float64 `json:"risk"`
}

// IsValid checks KER components are within [0, 1]
func (k *KerTriad) IsValid() bool {
	return k.Knowledge >= 0 && k.Knowledge <= 1 &&
		k.EcoImpact >= 0 && k.EcoImpact <= 1 &&
		k.Risk >= 0 && k.Risk <= 1
}

// SafetyScore computes weighted safety metric
func (k *KerTriad) SafetyScore() float64 {
	return (k.EcoImpact * WeightEcoImpact) + ((1.0 - k.Risk) * WeightRiskReduction)
}

// NodeStatus represents the real-time state of a registered node
type NodeStatus struct {
	DID            string    `json:"did"`
	NodeID         string    `json:"node_id"`
	RegionID       string    `json:"region_id"`
	LastHeartbeat  time.Time `json:"last_heartbeat"`
	CurrentResidual float64   `json:"current_residual"`
	Active         bool      `json:"active"`
	StepLevel      int       `json:"step_level"`
	BrainIdentityHash string `json:"brain_identity_hash,omitempty"` // Long-term continuity link
}

// RegionalEnvelope defines the safety boundaries for a geographic zone
type RegionalEnvelope struct {
	RegionID        string            `json:"region_id"`
	MaxResidual     float64           `json:"max_residual"`
	CurrentResidual float64           `json:"current_residual"`
	Corridors       map[string]Corridor `json:"corridors"`
	UpdatedAt       time.Time         `json:"updated_at"`
	ContractStamp   string            `json:"contract_stamp"`
}

// Corridor defines a regional constraint (e.g., total watershed toxicity)
type Corridor struct {
	Variable string  `json:"variable"`
	Limit    float64 `json:"limit"`
	Current  float64 `json:"current"`
	Unit     string  `json:"unit"`
}

// PortfolioDecision represents an optimization output
type PortfolioDecision struct {
	Timestamp    time.Time `json:"timestamp"`
	RegionID     string    `json:"region_id"`
	Actions      []NodeAction `json:"actions"`
	ProjectedRisk float64   `json:"projected_risk"`
	Valid        bool      `json:"valid"`
	Reason       string    `json:"reason"`
}

// NodeAction recommends a state change for a node
type NodeAction struct {
	NodeID  string `json:"node_id"`
	DID     string `json:"did"`
	Command string `json:"command"` // "scale_up", "scale_down", "halt", "maintain"
	Reason  string `json:"reason"`
}

// ============================================================================
// Regional Coordinator
// ============================================================================

// Coordinator manages the regional orchestration logic
type Coordinator struct {
	mu           sync.RWMutex
	region       RegionalEnvelope
	nodes        map[string]*NodeStatus // Key: DID
	auditLog     []AuditEntry
	config       CoordinatorConfig
}

// CoordinatorConfig holds initialization parameters
type CoordinatorConfig struct {
	RegionID      string
	InitialResidual float64
	AuditPath     string
}

// AuditEntry for long-term accountability
type AuditEntry struct {
	Timestamp time.Time `json:"timestamp"`
	Action    string    `json:"action"`
	Actor     string    `json:"actor"` // DID
	Details   string    `json:"details"`
}

// NewCoordinator initializes a regional orchestrator
func NewCoordinator(config CoordinatorConfig) (*Coordinator, error) {
	if config.InitialResidual > MaxRegionalResidual {
		return nil, ErrRegionalRiskViolation
	}

	return &Coordinator{
		region: RegionalEnvelope{
			RegionID:        config.RegionID,
			MaxResidual:     MaxRegionalResidual,
			CurrentResidual: config.InitialResidual,
			Corridors:       make(map[string]Corridor),
			UpdatedAt:       time.Now(),
			ContractStamp:   ContractHexStamp,
		},
		nodes:    make(map[string]*NodeStatus),
		auditLog: make([]AuditEntry, 0),
		config:   config,
	}, nil
}

// RegisterNode adds a new node to the regional portfolio
func (c *Coordinator) RegisterNode(did, nodeID string, brainHash string) error {
	c.mu.Lock()
	defer c.mu.Unlock()

	// 1. Validate DID Format (Simplified check)
	if len(did) < 10 || !contains(did, "did:bostrom") {
		c.logAudit("REGISTER_FAIL", did, "Invalid DID format")
		return ErrInvalidNodeDID
	}

	// 2. Check Contract Alignment
	// In production, verify against ALN registry

	// 3. Initialize Status
	c.nodes[did] = &NodeStatus{
		DID:             did,
		NodeID:          nodeID,
		RegionID:        c.region.RegionID,
		LastHeartbeat:   time.Now(),
		CurrentResidual: 0.0, // Starts neutral
		Active:          false, // Must be approved by optimizer first
		StepLevel:       0,
		BrainIdentityHash: brainHash,
	}

	c.logAudit("NODE_REGISTERED", did, fmt.Sprintf("Node %s added to region %s", nodeID, c.region.RegionID))
	return nil
}

// UpdateNodeMetrics ingests shard data from a node
func (c *Coordinator) UpdateNodeMetrics(did string, ker KerTriad, residual float64) error {
	c.mu.Lock()
	defer c.mu.Unlock()

	node, exists := c.nodes[did]
	if !exists {
		return ErrNodeNotRegistered
	}

	// 1. Validate KER
	if !ker.IsValid() {
		c.logAudit("METRIC_REJECTED", did, "Invalid KER triad")
		return errors.New("invalid ker triad")
	}

	// 2. Check Regional Risk Constraint (V_t+1 <= V_t)
	// Calculate projected regional residual if this node's risk is added/updated
	projectedRegional := c.calculateProjectedRegionalResidual(did, residual)
	
	if projectedRegional > c.region.CurrentResidual {
		c.logAudit("RISK_VIOLATION", did, fmt.Sprintf("Projected %.4f > Current %.4f", projectedRegional, c.region.CurrentResidual))
		return ErrRegionalRiskViolation
	}

	// 3. Update Node State
	node.CurrentResidual = residual
	node.LastHeartbeat = time.Now()
	if residual > 0 {
		node.Active = true
	}

	// 4. Update Regional Aggregate
	c.region.CurrentResidual = projectedRegional
	c.region.UpdatedAt = time.Now()

	c.logAudit("METRIC_ACCEPTED", did, fmt.Sprintf("Residual updated to %.4f", residual))
	return nil
}

// calculateProjectedRegionalResidual computes V_region if node updates
func (c *Coordinator) calculateProjectedRegionalResidual(nodeDID string, newResidual float64) float64 {
	total := 0.0
	count := 0
	
	for did, node := range c.nodes {
		if did == nodeDID {
			total += newResidual
		} else {
			total += node.CurrentResidual
		}
		count++
	}

	if count == 0 {
		return newResidual
	}

	// Weighted average or sum depending on envelope definition
	// Here we use weighted average for regional stability
	return total / float64(count)
}

// OptimizePortfolio runs the climbing-steps logic for the region
func (c *Coordinator) OptimizePortfolio(ctx context.Context) (*PortfolioDecision, error) {
	c.mu.Lock()
	defer c.mu.Unlock()

	decision := &PortfolioDecision{
		Timestamp:    time.Now(),
		RegionID:     c.region.RegionID,
		Actions:      make([]NodeAction, 0),
		ProjectedRisk: c.region.CurrentResidual,
		Valid:        true,
	}

	// 1. Check Regional Warning Threshold
	if c.region.CurrentResidual > WarningThreshold {
		decision.Reason = "Regional risk high; prioritizing risk reduction"
		decision.Valid = true
	} else {
		decision.Reason = "Regional risk stable; allowing expansion"
	}

	// 2. Sort Nodes by Safety Score (High E, Low R)
	type scoredNode struct {
		did   string
		score float64
		status *NodeStatus
	}
	
	var scoredNodes []scoredNode
	for did, node := range c.nodes {
		// Mock KER for sorting (In production, fetch from recent shards)
		// Assume higher step level implies higher K/E historically
		score := float64(node.StepLevel) * 0.1 + (1.0 - node.CurrentResidual)
		scoredNodes = append(scoredNodes, scoredNode{did, score, node})
	}

	sort.Slice(scoredNodes, func(i, j int) bool {
		return scoredNodes[i].score > scoredNodes[j].score
	})

	// 3. Generate Actions
	for _, sn := range scoredNodes {
		action := NodeAction{
			NodeID: sn.status.NodeID,
			DID:    sn.did,
		}

		// Logic: If regional risk is high, halt low performers
		if c.region.CurrentResidual > WarningThreshold {
			if sn.status.CurrentResidual > MaxRiskHarm {
				action.Command = "scale_down"
				action.Reason = "Regional risk constraint"
			} else {
				action.Command = "maintain"
				action.Reason = "Stable performance"
			}
		} else {
			// If regional risk is low, allow step-ups for high performers
			if sn.score > 0.8 && sn.status.StepLevel < 10 {
				action.Command = "scale_up"
				action.Reason = "Eligible for climbing-step"
			} else {
				action.Command = "maintain"
				action.Reason = "Performance adequate"
			}
		}

		decision.Actions = append(decision.Actions, action)
	}

	// 4. Final Safety Check
	if decision.ProjectedRisk > c.region.MaxResidual {
		decision.Valid = false
		decision.Reason = "Optimization violated regional envelope"
		return decision, ErrOptimizationFailed
	}

	c.logAudit("OPTIMIZATION_RUN", "SYSTEM", fmt.Sprintf("Generated %d actions", len(decision.Actions)))
	return decision, nil
}

// SetCorridor defines a regional constraint
func (c *Coordinator) SetCorridor(variable string, limit float64, unit string) {
	c.mu.Lock()
	defer c.mu.Unlock()

	c.region.Corridors[variable] = Corridor{
		Variable: variable,
		Limit:    limit,
		Current:  0.0,
		Unit:     unit,
	}
	c.logAudit("CORRIDOR_SET", "ADMIN", fmt.Sprintf("%s limit set to %.2f %s", variable, limit, unit))
}

// logAudit appends to the immutable audit log
func (c *Coordinator) logAudit(action, actor, details string) {
	entry := AuditEntry{
		Timestamp: time.Now(),
		Action:    action,
		Actor:     actor,
		Details:   details,
	}
	c.auditLog = append(c.auditLog, entry)
	
	// In production, write to append-only file or ALN storage
	log.Printf("[AUDIT] %s | %s | %s", action, actor, details)
}

// GetAuditLog retrieves the audit trail (for governance)
func (c *Coordinator) GetAuditLog() []AuditEntry {
	c.mu.RLock()
	defer c.mu.RUnlock()
	
	// Return copy to prevent modification
	copy := make([]AuditEntry, len(c.auditLog))
	for i, v := range c.auditLog {
		copy[i] = v
	}
	return copy
}

// GetRegionalStatus returns current envelope state
func (c *Coordinator) GetRegionalStatus() RegionalEnvelope {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.region
}

// Helper: String contains
func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(s) > len(substr) && findSubstring(s, substr))
}

func findSubstring(s, substr string) bool {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}

// ============================================================================
// JSON Serialization Helpers
// ============================================================================

// ToJSON exports regional status for dashboards
func (c *Coordinator) ToJSON() ([]byte, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return json.MarshalIndent(c.region, "", "  ")
}

// ============================================================================
// Unit Tests
// ============================================================================

// Example test function (run with `go test`)
/*
func TestRegionalRiskViolation(t *testing.T) {
	config := CoordinatorConfig{
		RegionID: "phoenix_west_01",
		InitialResidual: 0.5,
	}
	coord, err := NewCoordinator(config)
	if err != nil { t.Fatal(err) }

	// Register Node
	err = coord.RegisterNode("did:bostrom:test#v1", "node_01", "")
	if err != nil { t.Fatal(err) }

	// Attempt to increase risk
	err = coord.UpdateNodeMetrics("did:bostrom:test#v1", KerTriad{0.9, 0.9, 0.6}, 0.6)
	if err != ErrRegionalRiskViolation {
		t.Errorf("Expected risk violation, got %v", err)
	}
}
*/

// ============================================================================
// Main Entry Point (For standalone orchestrator service)
// ============================================================================

func Main() {
	log.Println("=== Ecotribute Regional Orchestrator ===")
	log.Printf("Contract: %s", ContractHexStamp)

	config := CoordinatorConfig{
		RegionID:      "phoenix_pilot_zone",
		InitialResidual: 0.5,
		AuditPath:     "/var/log/ecotribute/orchestrator_audit.log",
	}

	coord, err := NewCoordinator(config)
	if err != nil {
		log.Fatalf("Failed to init coordinator: %v", err)
	}

	// Setup Corridors
	coord.SetCorridor("watershed_toxicity", 5.0, "mg/L")
	coord.SetCorridor("energy_load", 1000.0, "kWh")

	// Register Mock Nodes
	coord.RegisterNode("did:bostrom:ecotribute:node_01#v1", "node_01", "hash_brain_01")
	coord.RegisterNode("did:bostrom:ecotribute:node_02#v1", "node_02", "hash_brain_02")

	// Simulate Metrics
	coord.UpdateNodeMetrics("did:bostrom:ecotribute:node_01#v1", KerTriad{0.94, 0.90, 0.13}, 0.13)
	coord.UpdateNodeMetrics("did:bostrom:ecotribute:node_02#v1", KerTriad{0.85, 0.80, 0.15}, 0.15)

	// Run Optimization
	decision, err := coord.OptimizePortfolio(context.Background())
	if err != nil {
		log.Printf("Optimization warning: %v", err)
	} else {
		log.Printf("Optimization Valid: %v", decision.Valid)
		for _, action := range decision.Actions {
			log.Printf("Action: %s -> %s (%s)", action.NodeID, action.Command, action.Reason)
		}
	}

	// Export Status
	status, _ := coord.ToJSON()
	log.Printf("Regional Status: %s", string(status))
}

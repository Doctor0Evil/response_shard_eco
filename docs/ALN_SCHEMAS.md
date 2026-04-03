# Cyboquatic ALN/CSV Schema Specification

> **Status:** `ACTIVE` | **Version:** `1.0.0-rc.1` | **Governance:** `Econet-Constellation`

## 1. Overview & Design Principles

This document defines the canonical schemas for data interchange between Cyboquatic subsystems. All data is serialized as **RFC4180-compliant CSV** (Auditable Log Notation - ALN) or validated JSON.

### Core Principles
1.  **No Corridor, No Build:** Every shard must include a `corridor_version` field. Shards missing this are rejected by CI pipelines.
2.  **Strict Typing:** Fields map directly to Rust newtypes (`RiskCoord`, `JoulesPerKg`). Implicit conversions are forbidden.
3.  **Lyapunov Invariant:** Shards containing state evolution must include `lyapunov_residual` and pass \(V_{t+1} \le V_t\) checks.
4.  **AI-Chat Readability:** Schemas are designed for LLM-based validation agents. Field names are deterministic and semantic.

---

## 2. Versioning Policy

Schemas follow **Semantic Versioning for Data**:
- **MAJOR**: Breaking change (field removal, type change, constraint tightening). Requires migration script.
- **MINOR**: Additive change (new optional field). Backward compatible.
- **PATCH**: Documentation updates or typo fixes. No structural change.

**Rule:** A shard is valid if its `corridor_version` matches the active governance table. CI rejects shards referencing deprecated versions.

---

## 3. Core Schemas

### 3.1. `cyboquatic.energy_mass_purification.v1`

**Purpose:** Output from `EnergyMassKernel`. Quantifies efficiency of contaminant removal.

| Field Name | Rust Type | CSV Type | Constraints | Description |
|---|---|---|---|---|
| `timestamp_ms` | `u64` | `Integer` | `> 0` | Unix epoch timestamp of measurement. |
| `contaminant_id` | `String` | `String` | `^[a-zA-Z0-9_-]+$` | Unique identifier for the pollutant. |
| `c_in` | `f64` | `Float` | `>= 0.0` | Influent concentration (mg/L). |
| `c_out` | `f64` | `Float` | `>= 0.0` | Effluent concentration (mg/L). |
| `flow_rate` | `f64` | `Float` | `> 0.0` | Volumetric flow rate (L/s). |
| `energy_joules` | `f64` | `Float` | `>= 0.0` | Total energy input for the cycle. |
| `mass_removed_kg` | `f64` | `Float` | `>= 0.0` | Computed mass of contaminant removed. |
| `jpk_value` | `f64` | `Float` | `>= 0.0` | Joules per Kilogram removed. Efficiency metric. |
| `risk_jpk` | `RiskCoord` | `Float` | `[0.0, 1.0]` | Normalized risk of J/Kg based on corridors. |
| `lyapunov_residual` | `f64` | `Float` | `>= 0.0` | Current system risk state (\(V_t\)). |
| `ker_k` | `f64` | `Float` | `[0.0, 1.0]` | Knowledge-Factor score. |
| `ker_e` | `f64` | `Float` | `[0.0, 1.0]` | Eco-Impact score. |
| `ker_r` | `f64` | `Float` | `[0.0, 1.0]` | Risk-of-Harm score. |
| `corridor_version` | `String` | `String` | `semver` | Version of corridor bands used for normalization. |

**Example Shard (CSV):**
```csv
timestamp_ms,contaminant_id,c_in,c_out,flow_rate,energy_joules,mass_removed_kg,jpk_value,risk_jpk,lyapunov_residual,ker_k,ker_e,ker_r,corridor_version
1698765432000,PFAS-001,150.0,12.5,4.2,8500.0,0.005775,1471.86,0.65,0.142,0.94,0.91,0.12,v2026.04
```

### 3.2. `cyboquatic.ker_governance.v1`

**Purpose:** Output from `KERGovernanceShard`. Used for CI gating and lane promotion.

| Field Name | Type | Constraints | Description |
|---|---|---|---|
| `shard_id` | `String` | `uuid_v4` | Unique identifier for this governance record. |
| `timestamp_ms` | `u64` | `> 0` | Evaluation timestamp. |
| `k` | `f64` | `[0.0, 1.0]` | Knowledge-Factor. |
| `e` | `f64` | `[0.0, 1.0]` | Eco-Impact. |
| `r` | `f64` | `[0.0, 1.0]` | Risk-of-Harm. |
| `lane` | `String` | `enum` | `production`, `research`, or `blocked`. |
| `rolling_mean_k` | `f64?` | `[0.0, 1.0]` | Optional: Windowed average of K. |
| `ema_k` | `f64?` | `[0.0, 1.0]` | Optional: Exponential Moving Average of K. |
| `thresholds` | `JSON` | `struct` | Serialized `KERThresholds` used for evaluation. |

**Example Shard (JSON):**
```json
{
  "shard_id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp_ms": 1698765432100,
  "k": 0.94,
  "e": 0.91,
  "r": 0.12,
  "lane": "production",
  "rolling_mean_k": 0.935,
  "ema_k": 0.938,
  "thresholds": {
    "k_min_production": 0.90,
    "e_min_production": 0.90,
    "r_max_production": 0.13
  }
}
```

### 3.3. `cyboquatic.fog_routing.v1`

**Purpose:** Output from `FogRouter`. Auditable routing decisions.

| Field Name | Type | Constraints | Description |
|---|---|---|---|
| `workload_id` | `String` | `non-empty` | ID of the routed workload. |
| `decision` | `String` | `enum` | `accepted`, `rerouted`, `rejected`, `deferred`. |
| `selected_node_id` | `String?` | `optional` | ID of the winning node (null if rejected). |
| `candidate_count` | `u32` | `> 0` | Total nodes considered. |
| `survivors_count` | `u32` | `>= 0` | Nodes that passed hard-gate predicates. |
| `rationale` | `String` | `non-empty` | Reason for decision (e.g., "ker_lane_blocked"). |
| `governance_hash` | `String` | `sha256_hex` | Cryptographic hash of shard + corridor state. |

---

## 4. Validation Rules & Invariants

### 4.1. RFC4180 Compliance

Fields must be comma-separated. Fields containing `,`, `CR`, `LF`, or `"` MUST be enclosed in double quotes. Embedded double quotes MUST be escaped as `""`. Line endings MUST be `CRLF` (`\r\n`).

### 4.2. Mathematical Invariants

| Invariant | Formula | Enforcement |
|---|---|---|
| **Lyapunov Stability** | \(V_{t+1} \le V_t + \epsilon\) | CI Gate (`ci_econet_gate.ps1`) |
| **Risk Bounds** | \(r_x \in [0, 1]\) | `RiskCoord::new()` validation |
| **KER Sum** | \(K + E + R \le 3.0\) | Implied by bounds, checked in CI |
| **Energy/Mass** | \(J/kg \ge 0\) | `JoulesPerKg::new()` check |

### 4.3. AI-Chat Validation Pattern

LLM agents can validate shards using this prompt structure:

> Validate the following ALN shard against `cyboquatic.energy_mass_purification.v1`. Check that all required fields are present, types match, risk coordinates are in [0,1], and `lyapunov_residual` is non-increasing compared to the previous step.

---

## 5. Interoperability Matrix

| Language | Type Mapping | Serialization Notes |
|---|---|---|
| Rust | `serde`, `csv`, `thiserror` | Primary source of truth. `#[derive(Serialize)]` enforces schema. |
| C / C++ | `struct`, `double`, `char*` | Use `#pragma pack` or `__attribute__((aligned(8)))` for binary FFI. Prefer CSV parsing via `libcsv`. |
| Lua | `table`, `number`, `string` | Tables map 1:1 to JSON. Use `dkjson` for robust parsing. No FFI for actuation. |
| Kotlin | `data class`, `Double`, `String` | Use `kotlinx.serialization`. `@SerialName` must match CSV headers exactly. |
| PowerShell | `[PSCustomObject]`, `[double]` | `ConvertFrom-Csv` handles RFC4180 natively. Validate types post-parse. |

---

## 6. Audit & Governance

Every shard emitted by a production node must include:
1. `governance_hash`: SHA-256 of the shard content + `corridor_version` + `timestamp`.
2. `did_signer`: Decentralized Identifier of the operator or automated agent responsible.
3. `audit_trail_id`: Reference to the CI run or calibration event that authorized this shard.

Verification command:

```bash
# Verify hash integrity of a shard against the corridor table
echo -n "${shard_data}${corridor_version}${timestamp}" | sha256sum
```

Shards failing hash verification are automatically flagged in the `eco_infra-governance` ledger and trigger a `Research` lane demotion for the affected node.

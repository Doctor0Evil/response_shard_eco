-- ============================================================================
-- Ecotribute Central Ledger Schema
-- Module: response_shard_eco/src/sql/schema/ecotribute_ledger.sql
-- Version: 1.0.0 (ALN Contract Hex: 0x7f8a9b)
-- 
-- Purpose:
--   Defines the persistent storage layer for the Ecotribute ecosystem.
--   Mirrors ALN contract state for off-chain analytics and audit.
--   Enforces data integrity for KER triads, Residual Risk, and Bostrom DIDs.
--   Supports long-term continuity (20-50 years) via immutable audit logs.
--
-- Safety Invariants:
--   1. KER components constrained to [0.0, 1.0].
--   2. Bostrom DID format validation via CHECK constraints.
--   3. Immutable audit trails for governance accountability.
--   4. Regional envelope tracking for cross-node coordination.
--
-- Database Target: PostgreSQL 14+ (for JSONB, UUID, Partitioning support)
-- ============================================================================

-- ----------------------------------------------------------------------------
-- 1. Extensions & Configuration
-- ----------------------------------------------------------------------------

-- Enable UUID generation
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
-- Enable cryptographic functions for hashing
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
-- Enable temporal table support (if available) or manual versioning
CREATE EXTENSION IF NOT EXISTS "btree_gist";

-- Set timezone to UTC for all timestamp operations
SET timezone = 'UTC';

-- ----------------------------------------------------------------------------
-- 2. Custom Enumerations
-- ----------------------------------------------------------------------------

-- Time window types for progressive timing (5m, 24h, 90d)
CREATE TYPE window_type AS ENUM (
    'SHORT',      -- 15 minutes
    'DAILY',      -- 24 hours
    'QUARTERLY',  -- 90 days
    'YEARLY'      -- 365 days (for long-term reporting)
);

-- Reward event types (Mint/Burn)
CREATE TYPE reward_type AS ENUM (
    'MINT',       -- Eco-wealth creation
    'BURN',       -- Eco-wealth destruction (regression penalty)
    'TRANSFER'    -- Governance transfer
);

-- Node operational status
CREATE TYPE node_status AS ENUM (
    'REGISTERED',
    'ACTIVE',
    'MAINTENANCE',
    'HALTED',     -- Safety gate triggered
    'DEREGISTERED'
);

-- Audit action categories
CREATE TYPE audit_action AS ENUM (
    'SHARD_SUBMIT',
    'REWARD_CLAIM',
    'STEP_UP',
    'CORRIDOR_UPDATE',
    'SAFETY_VIOLATION',
    'IDENTITY_LINK',
    'SYSTEM_CONFIG'
);

-- ----------------------------------------------------------------------------
-- 3. Core Identity & Contract Tables
-- ----------------------------------------------------------------------------

-- Stores Bostrom DID identities (Human or Agent)
CREATE TABLE identities (
    did                 VARCHAR(256) PRIMARY KEY,
    public_key_hash     VARCHAR(64) NOT NULL,
    created_at          TIMESTAMPTZ DEFAULT NOW(),
    last_active         TIMESTAMPTZ DEFAULT NOW(),
    brain_identity_hash VARCHAR(64), -- Optional link for Augmented Citizen status
    is_verified         BOOLEAN DEFAULT FALSE,
    contract_version    INTEGER NOT NULL DEFAULT 1,
    hex_stamp           VARCHAR(20) NOT NULL DEFAULT '0x7f8a9b',
    
    CONSTRAINT chk_did_format CHECK (did LIKE 'did:bostrom:%'),
    CONSTRAINT chk_hex_stamp CHECK (hex_stamp LIKE '0x%')
);

-- Stores ALN Contract Versions for versioning governance
CREATE TABLE contracts (
    contract_id         UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    hex_stamp           VARCHAR(20) UNIQUE NOT NULL,
    version             INTEGER NOT NULL,
    aln_address         VARCHAR(64),
    deployed_at         TIMESTAMPTZ DEFAULT NOW(),
    is_active           BOOLEAN DEFAULT TRUE,
    spec_hash           VARCHAR(64) NOT NULL, -- Hash of contract source code
    
    CONSTRAINT chk_version UNIQUE (version, hex_stamp)
);

-- ----------------------------------------------------------------------------
-- 4. Node & Regional Infrastructure
-- ----------------------------------------------------------------------------

-- Physical or Logical Nodes (Cyboquatic Nodes, MAR Cells, etc.)
CREATE TABLE nodes (
    node_id             VARCHAR(128) PRIMARY KEY,
    owner_did           VARCHAR(256) NOT NULL REFERENCES identities(did),
    region_id           VARCHAR(128) NOT NULL,
    status              node_status DEFAULT 'REGISTERED',
    current_step_level  INTEGER DEFAULT 0,
    consecutive_safe_windows INTEGER DEFAULT 0,
    total_shards        BIGINT DEFAULT 0,
    total_eco_wealth    DECIMAL(18, 6) DEFAULT 0.0,
    registered_at       TIMESTAMPTZ DEFAULT NOW(),
    last_heartbeat      TIMESTAMPTZ DEFAULT NOW(),
    
    CONSTRAINT chk_step_level CHECK (current_step_level >= 0)
);

CREATE INDEX idx_nodes_owner ON nodes(owner_did);
CREATE INDEX idx_nodes_region ON nodes(region_id);
CREATE INDEX idx_nodes_status ON nodes(status);

-- Regional Envelopes (Geographic Safety Bounds)
CREATE TABLE regional_envelopes (
    region_id           VARCHAR(128) PRIMARY KEY,
    max_residual        DECIMAL(5, 4) NOT NULL DEFAULT 1.0,
    current_residual    DECIMAL(5, 4) NOT NULL DEFAULT 0.0,
    updated_at          TIMESTAMPTZ DEFAULT NOW(),
    contract_hex_stamp  VARCHAR(20) NOT NULL,
    
    CONSTRAINT chk_residual_range CHECK (current_residual >= 0.0 AND current_residual <= 1.0)
);

-- Safety Corridor Definitions (Toxicity, CPU, etc.)
CREATE TABLE corridors (
    corridor_id         UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    variable            VARCHAR(128) NOT NULL,
    region_id           VARCHAR(128) REFERENCES regional_envelopes(region_id),
    min_value           DECIMAL(18, 6) NOT NULL,
    max_value           DECIMAL(18, 6) NOT NULL,
    weight              DECIMAL(5, 4) DEFAULT 1.0,
    unit                VARCHAR(32),
    version             VARCHAR(32) NOT NULL,
    valid_from          TIMESTAMPTZ DEFAULT NOW(),
    valid_to            TIMESTAMPTZ, -- NULL means currently active
    
    CONSTRAINT chk_corridor_range CHECK (min_value < max_value),
    CONSTRAINT chk_weight_range CHECK (weight >= 0.0 AND weight <= 1.0)
);

CREATE INDEX idx_corridors_region ON corridors(region_id);
CREATE INDEX idx_corridors_variable ON corridors(variable);

-- ----------------------------------------------------------------------------
-- 5. Core Data: Shards & Windows
-- ----------------------------------------------------------------------------

-- ResponseShards (The fundamental unit of knowledge production)
-- Partitioned by creation date for performance over 20-50 years
CREATE TABLE shards (
    shard_id            VARCHAR(128) PRIMARY KEY,
    producer_did        VARCHAR(256) NOT NULL REFERENCES identities(did),
    node_id             VARCHAR(128) NOT NULL REFERENCES nodes(node_id),
    topic_tag           VARCHAR(128),
    
    -- KER Triad
    knowledge           DECIMAL(5, 4) NOT NULL,
    eco_impact          DECIMAL(5, 4) NOT NULL,
    risk                DECIMAL(5, 4) NOT NULL,
    
    -- Risk State
    residual_current    DECIMAL(5, 4) NOT NULL,
    residual_previous   DECIMAL(5, 4) NOT NULL,
    
    -- Metadata
    window_id           VARCHAR(128),
    created_at          TIMESTAMPTZ DEFAULT NOW(),
    contract_hex_stamp  VARCHAR(20) NOT NULL,
    eco_impact_score    DECIMAL(18, 6),
    signature           VARCHAR(256), -- Cryptographic proof
    
    -- Constraints
    CONSTRAINT chk_ker_k CHECK (knowledge >= 0.0 AND knowledge <= 1.0),
    CONSTRAINT chk_ker_e CHECK (eco_impact >= 0.0 AND eco_impact <= 1.0),
    CONSTRAINT chk_ker_r CHECK (risk >= 0.0 AND risk <= 1.0),
    CONSTRAINT chk_residual_non_increase CHECK (residual_current <= residual_previous + 0.0001)
);

-- Partitioning Example (PostgreSQL 10+)
-- CREATE TABLE shards_2024 PARTITION OF shards FOR VALUES FROM ('2024-01-01') TO ('2025-01-01');

CREATE INDEX idx_shards_did ON shards(producer_did);
CREATE INDEX idx_shards_node ON shards(node_id);
CREATE INDEX idx_shards_time ON shards(created_at DESC);
CREATE INDEX idx_shards_window ON shards(window_id);

-- Aggregated Time Windows (For Climbing-Steps Logic)
CREATE TABLE windows (
    window_id           VARCHAR(128) PRIMARY KEY,
    window_type         window_type NOT NULL,
    node_id             VARCHAR(128) NOT NULL REFERENCES nodes(node_id),
    did                 VARCHAR(256) NOT NULL REFERENCES identities(did),
    start_time          TIMESTAMPTZ NOT NULL,
    end_time            TIMESTAMPTZ NOT NULL,
    
    -- Aggregated KER
    avg_knowledge       DECIMAL(5, 4),
    avg_eco_impact      DECIMAL(5, 4),
    avg_risk            DECIMAL(5, 4),
    shard_count         INTEGER DEFAULT 0,
    
    -- Risk State
    residual_open       DECIMAL(5, 4),
    residual_close      DECIMAL(5, 4),
    
    -- Validation
    is_valid            BOOLEAN DEFAULT FALSE,
    eco_wealth_minted   DECIMAL(18, 6) DEFAULT 0.0,
    step_level          INTEGER DEFAULT 0,
    
    CONSTRAINT chk_window_time CHECK (end_time > start_time),
    CONSTRAINT chk_window_residual CHECK (residual_close <= residual_open + 0.0001)
);

CREATE INDEX idx_windows_did ON windows(did);
CREATE INDEX idx_windows_time ON windows(start_time, end_time);
CREATE INDEX idx_windows_valid ON windows(is_valid);

-- ----------------------------------------------------------------------------
-- 6. Eco-Wealth & Rewards
-- ----------------------------------------------------------------------------

-- Token Minting/Burning Events
CREATE TABLE rewards (
    event_id            VARCHAR(128) PRIMARY KEY,
    did                 VARCHAR(256) NOT NULL REFERENCES identities(did),
    window_id           VARCHAR(128) REFERENCES windows(window_id),
    event_type          reward_type NOT NULL,
    amount              DECIMAL(18, 6) NOT NULL,
    timestamp           TIMESTAMPTZ DEFAULT NOW(),
    step_level          INTEGER DEFAULT 0,
    reason              TEXT,
    aln_tx_hash         VARCHAR(64), -- Link to on-chain transaction
    
    CONSTRAINT chk_amount_positive CHECK (amount >= 0.0)
);

CREATE INDEX idx_rewards_did ON rewards(did);
CREATE INDEX idx_rewards_time ON rewards(timestamp DESC);
CREATE INDEX idx_rewards_type ON rewards(event_type);

-- ----------------------------------------------------------------------------
-- 7. Audit & Governance
-- ----------------------------------------------------------------------------

-- Immutable Audit Log (Append-Only)
CREATE TABLE audit_log (
    log_id              BIGSERIAL PRIMARY KEY,
    timestamp           TIMESTAMPTZ DEFAULT NOW(),
    action              audit_action NOT NULL,
    actor_did           VARCHAR(256) REFERENCES identities(did),
    target_id           VARCHAR(256), -- Node ID, Window ID, etc.
    details             JSONB,
    signature           VARCHAR(256),
    ip_hash             VARCHAR(64), -- Anonymized source
    
    CONSTRAINT chk_audit_immutable CHECK (TRUE) -- Logical placeholder for append-only policy
);

-- Prevent updates/deletes on audit log via trigger
CREATE OR REPLACE FUNCTION prevent_audit_modification()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'UPDATE' OR TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Audit log is immutable';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_audit_immutable
BEFORE UPDATE OR DELETE ON audit_log
FOR EACH ROW EXECUTE FUNCTION prevent_audit_modification();

CREATE INDEX idx_audit_time ON audit_log(timestamp DESC);
CREATE INDEX idx_audit_actor ON audit_log(actor_did);
CREATE INDEX idx_audit_action ON audit_log(action);

-- ----------------------------------------------------------------------------
-- 8. Analytics Views
-- ----------------------------------------------------------------------------

-- Node Performance Summary (For Dashboards)
CREATE VIEW node_performance_summary AS
SELECT 
    n.node_id,
    n.owner_did,
    n.current_step_level,
    n.total_shards,
    n.total_eco_wealth,
    COUNT(w.window_id) as total_windows,
    SUM(CASE WHEN w.is_valid THEN 1 ELSE 0 END) as valid_windows,
    AVG(w.avg_eco_impact) as avg_impact,
    AVG(w.avg_risk) as avg_risk,
    MAX(w.end_time) as last_active
FROM nodes n
LEFT JOIN windows w ON n.node_id = w.node_id
GROUP BY n.node_id, n.owner_did, n.current_step_level, n.total_shards, n.total_eco_wealth;

-- Regional Risk Heatmap (For Orchestrator)
CREATE VIEW regional_risk_heatmap AS
SELECT 
    r.region_id,
    r.current_residual,
    r.max_residual,
    COUNT(n.node_id) as active_nodes,
    AVG(w.avg_risk) as avg_node_risk,
    CASE 
        WHEN r.current_residual > 0.8 THEN 'CRITICAL'
        WHEN r.current_residual > 0.5 THEN 'WARNING'
        ELSE 'SAFE'
    END as risk_status
FROM regional_envelopes r
LEFT JOIN nodes n ON r.region_id = n.region_id
LEFT JOIN windows w ON n.node_id = w.node_id AND w.is_valid = TRUE
GROUP BY r.region_id, r.current_residual, r.max_residual;

-- ----------------------------------------------------------------------------
-- 9. Roles & Permissions
-- ----------------------------------------------------------------------------

-- Read-Only Access (Analysts, Dashboards)
CREATE ROLE ecotribute_reader;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO ecotribute_reader;
GRANT USAGE ON SCHEMA public TO ecotribute_reader;

-- Write Access (Nodes, Automation Scripts)
CREATE ROLE ecotribute_writer;
GRANT SELECT, INSERT ON shards, windows, audit_log TO ecotribute_writer;
GRANT SELECT, UPDATE ON nodes, regional_envelopes TO ecotribute_writer;
GRANT USAGE ON SEQUENCE audit_log_log_id_seq TO ecotribute_writer;

-- Admin Access (Governance, Contract Owners)
CREATE ROLE ecotribute_admin;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO ecotribute_admin;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO ecotribute_admin;

-- ----------------------------------------------------------------------------
-- 10. Initial Data Seeding (Optional)
-- ----------------------------------------------------------------------------

-- Insert Default Contract Version
INSERT INTO contracts (hex_stamp, version, is_active, spec_hash)
VALUES ('0x7f8a9b', 1, TRUE, sha256('ecosafety_gate.aln.v1'));

-- Insert Default Regional Envelope (Phoenix Pilot)
INSERT INTO regional_envelopes (region_id, max_residual, current_residual, contract_hex_stamp)
VALUES ('phoenix_pilot_zone', 1.0, 0.5, '0x7f8a9b');

-- Insert Default Corridors
INSERT INTO corridors (variable, region_id, min_value, max_value, weight, unit, version)
VALUES 
('toxicity', 'phoenix_pilot_zone', 0.0, 5.0, 0.5, 'mg/L', 'v1.0'),
('cpu_load', 'phoenix_pilot_zone', 0.0, 0.8, 0.3, 'percent', 'v1.0'),
('turbidity', 'phoenix_pilot_zone', 0.0, 10.0, 0.2, 'NTU', 'v1.0');

-- ============================================================================
-- End of Schema
-- ============================================================================

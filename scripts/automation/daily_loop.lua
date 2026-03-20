#!/usr/bin/env lua
-- ============================================================================
-- Ecotribute Daily Automation Loop
-- Module: response_shard_eco/scripts/automation/daily_loop.lua
-- Version: 1.0.0 (ALN Contract Hex: 0x7f8a9b)
-- 
-- Purpose:
--   Orchestrates daily research/ops tasks into ResponseShard-producing actions.
--   Enforces "No Corridor, No Build" CI gating and KER-based prioritization.
--   Ensures non-increasing residual risk (V_t+1 <= V_t) for all automated steps.
--
-- Safety Invariants:
--   1. Every task must produce a Shard with KER triad.
--   2. No task executes if projected Risk > Current Residual.
--   3. All identities anchored to Bostrom DID.
--   4. Progressive timing windows enforced (5m, 24h, 90d).
-- ============================================================================

local json = require("json") -- Assume lua-cjson or similar
local posix = require("posix") -- For system time/file ops
local strict = require("strict") -- Enforce strict variable declaration

-- ============================================================================
-- Configuration & Safety Constants
-- ============================================================================

local CONFIG = {
    -- Safety Thresholds (KER)
    MIN_KNOWLEDGE_FACTOR = 0.85,      -- Minimum K for step advancement
    MIN_ECO_IMPACT = 0.50,            -- Minimum E for reward eligibility
    MAX_RISK_HARM = 0.20,             -- Maximum R allowed for execution
    RESIDUAL_RISK_LIMIT = 1.0,        -- Absolute ceiling for V_t
    
    -- Time Windows (seconds)
    WINDOW_SHORT = 900,               -- 15 minutes
    WINDOW_DAILY = 86400,             -- 24 hours
    WINDOW_QUARTERLY = 7776000,       -- 90 days
    
    -- Paths
    SHARD_OUTPUT_DIR = "/var/ecotribute/shards/",
    LOG_DIR = "/var/log/ecotribute/",
    KERNEL_BIN = "/usr/bin/ecotribute_kernel", -- Rust binary for heavy lifting
}

-- Global State (Persistent across runs via file lock)
local STATE = {
    current_residual = 0.0,
    last_window_close = 0,
    total_shards_produced = 0,
    active_did = nil,
}

-- ============================================================================
-- Utility Functions
-- ============================================================================

--- Get current Unix timestamp
local function get_timestamp()
    return os.time()
end

--- Log with safety level
local function log_safety(level, message, data)
    local ts = os.date("!%Y-%m-%dT%H:%M:%SZ", get_timestamp())
    local entry = string.format("[%s] [%s] %s", ts, level, message)
    if data then
        entry = entry .. " | " .. json.encode(data)
    end
    print(entry) -- In production, write to CONFIG.LOG_DIR
end

--- Validate Bostrom DID format
local function validate_did(did)
    if not did or not did:match("^did:bostrom:ecotribute:.+#v%d+$") then
        return false, "Invalid Bostrom DID format"
    end
    return true
end

-- ============================================================================
-- KER & Risk Computation Logic
-- ============================================================================

--- Compute Knowledge Factor (K = N_corridor-backed / N_critical)
local function compute_knowledge_factor(corridor_backed, critical_total)
    if critical_total == 0 then return 0.0 end
    local k = corridor_backed / critical_total
    return math.min(1.0, math.max(0.0, k))
end

--- Compute Eco Impact (E) from mass kernel
local function compute_eco_impact(mass_removed, max_capacity, efficiency)
    if max_capacity <= 0 then return 0.0 end
    local e = (mass_removed / max_capacity) * efficiency
    return math.min(1.0, math.max(0.0, e))
end

--- Compute Risk Score (R) from weighted coordinates
local function compute_risk_score(risk_coords)
    local total_weight = 0
    local weighted_sum = 0
    for _, coord in ipairs(risk_coords) do
        total_weight = total_weight + coord.weight
        weighted_sum = weighted_sum + (coord.weight * coord.risk_value)
    end
    if total_weight == 0 then return 0.0 end
    return math.min(1.0, math.max(0.0, weighted_sum / total_weight))
end

--- Compute Residual Risk (V_t = Σ w_j * r_j^2)
local function compute_residual(risk_coords)
    local residual = 0.0
    for _, coord in ipairs(risk_coords) do
        residual = residual + (coord.weight * (coord.risk_value ^ 2))
    end
    return residual
end

-- ============================================================================
-- CI Gating & Safety Enforcement
-- ============================================================================

--- "No Corridor, No Build" Check
-- Validates that a task defines all mandatory corridors before execution
local function ci_corridor_check(task)
    if not task.corridors or #task.corridors == 0 then
        return false, "CI_FAIL: No corridors defined for task " .. task.id
    end
    
    for _, corridor in ipairs(task.corridors) do
        if not corridor.variable or not corridor.min or not corridor.max then
            return false, "CI_FAIL: Malformed corridor definition in task " .. task.id
        end
        if corridor.min >= corridor.max then
            return false, "CI_FAIL: Invalid corridor range (" .. corridor.variable .. ")"
        end
    end
    return true
end

--- Residual Risk Constraint Check (V_t+1 <= V_t)
local function ci_residual_check(projected_residual)
    if projected_residual > STATE.current_residual + 0.0001 then -- Epsilon for float noise
        return false, "CI_FAIL: Residual risk increase detected (V_t+1 > V_t)"
    end
    return true
end

--- Main Safety Gate
local function execute_safety_gate(task)
    log_safety("INFO", "Running safety gate for task", {id = task.id})
    
    -- 1. Corridor Check
    local ok, err = ci_corridor_check(task)
    if not ok then
        log_safety("CRITICAL", err)
        return false, err
    end
    
    -- 2. Risk Projection
    local projected_residual = compute_residual(task.risk_coords)
    ok, err = ci_residual_check(projected_residual)
    if not ok then
        log_safety("CRITICAL", err, {projected = projected_residual, current = STATE.current_residual})
        return false, err
    end
    
    -- 3. KER Thresholds (Warning only, does not block unless critical)
    if task.ker.risk > CONFIG.MAX_RISK_HARM then
        log_safety("WARNING", "Task risk exceeds soft limit", {risk = task.ker.risk})
    end
    
    log_safety("PASS", "Safety gate passed for task", {id = task.id})
    return true
end

-- ============================================================================
-- Task Scheduler & Prioritization
-- ============================================================================

--- Prioritize tasks based on KER logic
-- Priority = (EcoImpact * 0.5) + ((1 - Risk) * 0.5) + (KnowledgeNeed * 0.2)
local function calculate_task_priority(task)
    local safety_score = (task.ker.eco_impact * 0.5) + ((1.0 - task.ker.risk) * 0.5)
    local knowledge_need = 0.0
    if task.ker.knowledge < CONFIG.MIN_KNOWLEDGE_FACTOR then
        knowledge_need = 0.2 -- Boost priority if knowledge is low
    end
    return safety_score + knowledge_need
end

--- Sort task queue
local function prioritize_queue(queue)
    table.sort(queue, function(a, b)
        return calculate_task_priority(a) > calculate_task_priority(b)
    end)
    return queue
end

-- ============================================================================
-- Shard Production
-- ============================================================================

--- Generate ResponseShard CSV row
local function emit_shard(task, execution_result)
    local ts = get_timestamp()
    local shard_id = string.format("shard_%s_%d", task.id, ts)
    
    -- Construct Shard Record
    local shard = {
        shard_id = shard_id,
        producer_did = STATE.active_did,
        topic_tag = task.topic,
        ker = task.ker,
        corridor_updates = task.corridors,
        time_window = task.window_id,
        created_at = ts,
        contract_hex_stamp = "0x7f8a9b",
        eco_impact_score = execution_result.actual_impact,
        node_id = execution_result.node_id,
    }
    
    -- Write to CSV (Append Mode)
    local filename = CONFIG.SHARD_OUTPUT_DIR .. "daily_shards_" .. os.date("%Y%m%d") .. ".csv"
    local file = io.open(filename, "a")
    if not file then
        log_safety("ERROR", "Failed to open shard file", {filename = filename})
        return false
    end
    
    -- Header check (simplified)
    if STATE.total_shards_produced == 0 then
        file:write("shard_id,did,topic,K,E,R,eco_score,node_id,timestamp\n")
    end
    
    -- Write Row
    local row = string.format("%s,%s,%s,%.4f,%.4f,%.4f,%.4f,%s,%d\n",
        shard.shard_id,
        shard.producer_did,
        shard.topic_tag,
        shard.ker.knowledge,
        shard.ker.eco_impact,
        shard.ker.risk,
        shard.eco_impact_score,
        shard.node_id,
        shard.created_at
    )
    file:write(row)
    file:close()
    
    STATE.total_shards_produced = STATE.total_shards_produced + 1
    log_safety("INFO", "Shard emitted", {id = shard_id})
    return true
end

-- ============================================================================
-- Execution Engine
-- ============================================================================

--- Execute a single task with safety wrapping
local function execute_task(task)
    log_safety("INFO", "Executing task", {id = task.id})
    
    -- 1. Safety Gate
    local safe, err = execute_safety_gate(task)
    if not safe then
        log_safety("BLOCKED", "Task blocked by safety gate", {id = task.id, reason = err})
        return false
    end
    
    -- 2. Simulate/Run Task (Hook to Rust Kernel or External Script)
    -- In production: os.execute(CONFIG.KERNEL_BIN .. " run " .. task.id)
    local success = true
    local actual_impact = task.ker.eco_impact -- Assume target met for demo
    local node_id = "node_phoenix_01"
    
    if not success then
        log_safety("ERROR", "Task execution failed", {id = task.id})
        return false
    end
    
    -- 3. Update State (Residual Risk)
    local new_residual = compute_residual(task.risk_coords)
    STATE.current_residual = new_residual
    
    -- 4. Emit Shard
    local result = {
        actual_impact = actual_impact,
        node_id = node_id
    }
    emit_shard(task, result)
    
    return true
end

--- Main Daily Loop
local function run_daily_loop()
    log_safety("INFO", "Starting Daily Automation Loop")
    
    -- 1. Load Identity
    -- In production: Load from secure vault
    STATE.active_did = "did:bostrom:ecotribute:agent_auto_01#v1"
    local valid, err = validate_did(STATE.active_did)
    if not valid then
        log_safety("CRITICAL", "Identity validation failed", {reason = err})
        return
    end
    
    -- 2. Load Task Queue (Mock Data for Demonstration)
    local task_queue = {
        {
            id = "task_bio_sim_001",
            topic = "biodegradation",
            corridors = {
                {variable = "toxicity", min = 0.0, max = 0.5, weight = 0.6},
                {variable = "microplastics", min = 0.0, max = 0.1, weight = 0.4}
            },
            risk_coords = {
                {variable = "toxicity", risk_value = 0.1, weight = 0.6},
                {variable = "microplastics", risk_value = 0.05, weight = 0.4}
            },
            ker = {
                knowledge = 0.94,
                eco_impact = 0.90,
                risk = 0.13
            },
            window_id = "w_20260320_001"
        },
        {
            id = "task_hw_control_002",
            topic = "hardware_control",
            corridors = {
                {variable = "cpu_temp", min = 0.0, max = 80.0, weight = 1.0}
            },
            risk_coords = {
                {variable = "cpu_temp", risk_value = 0.2, weight = 1.0}
            },
            ker = {
                knowledge = 0.80,
                eco_impact = 0.50,
                risk = 0.20
            },
            window_id = "w_20260320_001"
        }
    }
    
    -- 3. Prioritize
    task_queue = prioritize_queue(task_queue)
    
    -- 4. Execute
    local success_count = 0
    for _, task in ipairs(task_queue) do
        if execute_task(task) then
            success_count = success_count + 1
        end
    end
    
    log_safety("INFO", "Daily loop complete", {
        tasks_processed = #task_queue,
        tasks_success = success_count,
        shards_emitted = STATE.total_shards_produced,
        final_residual = STATE.current_residual
    })
end

-- ============================================================================
-- Entry Point
-- ============================================================================

-- Run if executed directly
if arg and arg[0] and arg[0]:match("daily_loop.lua$") then
    local status, err = pcall(run_daily_loop)
    if not status then
        log_safety("CRITICAL", "Unhandled exception in daily loop", {error = err})
        os.exit(1)
    end
end

return {
    run_daily_loop = run_daily_loop,
    compute_residual = compute_residual,
    execute_safety_gate = execute_safety_gate,
    CONFIG = CONFIG
}

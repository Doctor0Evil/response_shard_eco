#!/usr/bin/env python3
# ============================================================================
# Ecotribute Eco-Wealth Tracker & Analytics Module
# Module: response_shard_eco/src/python/analysis/eco_wealth_tracker.py
# Version: 1.0.0 (ALN Contract Hex: 0x7f8a9b)
#
# Purpose:
#   Tracks eco-wealth gains, token minting events, and climbing-step progression.
#   Analyzes shard data from Rust kernel and C++ node simulators.
#   Validates KER thresholds over explicit time windows (5m, 24h, 90d).
#   Produces analytics dashboards for governance and reward distribution.
#
# Safety Invariants:
#   1. All rewards tied to verified eco-impact (CEIM mass kernels).
#   2. Step advancement requires N consecutive safe windows.
#   3. Residual risk V_t must be non-increasing across window runs.
#   4. Identity linkage via Bostrom DID for long-term continuity.
# ============================================================================

import csv
import json
import hashlib
import logging
from datetime import datetime, timedelta
from dataclasses import dataclass, field, asdict
from typing import Dict, List, Optional, Tuple, Any
from pathlib import Path
from enum import Enum
import statistics
import sqlite3
from contextlib import contextmanager

# ============================================================================
# Configuration & Constants
# ============================================================================

class WindowType(Enum):
    """Time window classifications for progressive timing."""
    SHORT = "short"           # 5-15 minutes
    DAILY = "daily"           # 24 hours
    QUARTERLY = "quarterly"   # 90 days

@dataclass
class Config:
    """Global configuration for eco-wealth tracking."""
    
    # KER Thresholds
    MIN_KNOWLEDGE_FACTOR: float = 0.85
    MIN_ECO_IMPACT: float = 0.50
    MAX_RISK_HARM: float = 0.20
    
    # Time Windows (seconds)
    WINDOW_SHORT: int = 900        # 15 minutes
    WINDOW_DAILY: int = 86400      # 24 hours
    WINDOW_QUARTERLY: int = 7776000  # 90 days
    
    # Climbing-Steps
    STEP_WINDOW_REQUIREMENT: int = 5  # N consecutive windows to step up
    STEP_MULTIPLIER_BASE: float = 1.0
    STEP_MULTIPLIER_INCREMENT: float = 0.1
    
    # Paths
    SHARD_INPUT_DIR: str = "/var/ecotribute/shards/"
    ANALYSIS_OUTPUT_DIR: str = "/var/ecotribute/analysis/"
    DATABASE_PATH: str = "/var/ecotribute/db/eco_wealth.db"
    
    # Tokenomics
    MINT_RATE_BASE: float = 1.0  # Base tokens per window
    BURN_REGRESSION_WINDOWS: int = 3  # Burn if E regresses for N windows
    
    # ALN Contract
    CONTRACT_HEX_STAMP: str = "0x7f8a9b"
    CONTRACT_VERSION: int = 1

# ============================================================================
# Data Structures
# ============================================================================

@dataclass
class KerTriad:
    """Knowledge-Eco-Risk triad for scoring."""
    knowledge: float
    eco_impact: float
    risk: float
    
    def valid(self) -> bool:
        """Validate all components in [0, 1] range."""
        return (0.0 <= self.knowledge <= 1.0 and
                0.0 <= self.eco_impact <= 1.0 and
                0.0 <= self.risk <= 1.0)
    
    def safety_score(self) -> float:
        """Compute composite safety score (higher is better)."""
        return (self.eco_impact * 0.5) + ((1.0 - self.risk) * 0.5)
    
    def meets_thresholds(self, config: Config) -> bool:
        """Check if triad meets minimum thresholds for rewards."""
        return (self.knowledge >= config.MIN_KNOWLEDGE_FACTOR and
                self.eco_impact >= config.MIN_ECO_IMPACT and
                self.risk <= config.MAX_RISK_HARM)

@dataclass
class ResponseShard:
    """Parsed ResponseShard from CSV output."""
    shard_id: str
    producer_did: str
    node_id: str
    topic_tag: str
    ker: KerTriad
    residual: float
    timestamp: int
    eco_impact_score: float
    contract_hex_stamp: str
    
    @classmethod
    def from_csv_row(cls, row: Dict[str, str]) -> 'ResponseShard':
        """Parse shard from CSV row dictionary."""
        return cls(
            shard_id=row['shard_id'],
            producer_did=row['did'],
            node_id=row['node_id'],
            topic_tag=row.get('topic', 'unknown'),
            ker=KerTriad(
                knowledge=float(row['K']),
                eco_impact=float(row['E']),
                risk=float(row['R'])
            ),
            residual=float(row.get('residual', 0.0)),
            timestamp=int(row['timestamp']),
            eco_impact_score=float(row.get('eco_score', 0.0)),
            contract_hex_stamp=row.get('contract_hex_stamp', '0x7f8a9b')
        )

@dataclass
class WindowRecord:
    """Aggregated metrics for a time window."""
    window_id: str
    window_type: WindowType
    start_time: int
    end_time: int
    node_id: str
    did: str
    shard_count: int
    avg_ker: KerTriad
    residual_open: float
    residual_close: float
    is_valid: bool
    eco_wealth_minted: float = 0.0
    step_level: int = 0

@dataclass
class NodeProfile:
    """Long-term profile for a node/identity."""
    did: str
    node_id: str
    total_shards: int = 0
    total_eco_wealth: float = 0.0
    current_step_level: int = 0
    consecutive_safe_windows: int = 0
    last_reward_time: int = 0
    brain_identity_hash: Optional[str] = None
    created_at: int = 0
    last_active: int = 0

@dataclass
class RewardEvent:
    """Token minting/burning event record."""
    event_id: str
    did: str
    window_id: str
    event_type: str  # "mint" or "burn"
    amount: float
    timestamp: int
    step_level: int
    reason: str

# ============================================================================
# Database Manager
# ============================================================================

class DatabaseManager:
    """SQLite database manager for eco-wealth tracking."""
    
    def __init__(self, db_path: str):
        self.db_path = Path(db_path)
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._init_schema()
    
    @contextmanager
    def get_connection(self):
        """Context manager for database connections."""
        conn = sqlite3.connect(str(self.db_path))
        conn.row_factory = sqlite3.Row
        try:
            yield conn
            conn.commit()
        except Exception as e:
            conn.rollback()
            raise e
        finally:
            conn.close()
    
    def _init_schema(self):
        """Initialize database schema."""
        with self.get_connection() as conn:
            conn.executescript("""
                CREATE TABLE IF NOT EXISTS nodes (
                    did TEXT PRIMARY KEY,
                    node_id TEXT NOT NULL,
                    total_shards INTEGER DEFAULT 0,
                    total_eco_wealth REAL DEFAULT 0.0,
                    current_step_level INTEGER DEFAULT 0,
                    consecutive_safe_windows INTEGER DEFAULT 0,
                    last_reward_time INTEGER DEFAULT 0,
                    brain_identity_hash TEXT,
                    created_at INTEGER DEFAULT (strftime('%s', 'now')),
                    last_active INTEGER DEFAULT 0
                );
                
                CREATE TABLE IF NOT EXISTS windows (
                    window_id TEXT PRIMARY KEY,
                    window_type TEXT NOT NULL,
                    start_time INTEGER NOT NULL,
                    end_time INTEGER NOT NULL,
                    node_id TEXT NOT NULL,
                    did TEXT NOT NULL,
                    shard_count INTEGER DEFAULT 0,
                    avg_knowledge REAL DEFAULT 0.0,
                    avg_eco_impact REAL DEFAULT 0.0,
                    avg_risk REAL DEFAULT 0.0,
                    residual_open REAL DEFAULT 0.0,
                    residual_close REAL DEFAULT 0.0,
                    is_valid INTEGER DEFAULT 0,
                    eco_wealth_minted REAL DEFAULT 0.0,
                    step_level INTEGER DEFAULT 0,
                    FOREIGN KEY (did) REFERENCES nodes(did)
                );
                
                CREATE TABLE IF NOT EXISTS rewards (
                    event_id TEXT PRIMARY KEY,
                    did TEXT NOT NULL,
                    window_id TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    amount REAL NOT NULL,
                    timestamp INTEGER NOT NULL,
                    step_level INTEGER DEFAULT 0,
                    reason TEXT,
                    FOREIGN KEY (did) REFERENCES nodes(did),
                    FOREIGN KEY (window_id) REFERENCES windows(window_id)
                );
                
                CREATE TABLE IF NOT EXISTS shards (
                    shard_id TEXT PRIMARY KEY,
                    producer_did TEXT NOT NULL,
                    node_id TEXT NOT NULL,
                    topic_tag TEXT,
                    knowledge REAL DEFAULT 0.0,
                    eco_impact REAL DEFAULT 0.0,
                    risk REAL DEFAULT 0.0,
                    residual REAL DEFAULT 0.0,
                    timestamp INTEGER NOT NULL,
                    eco_impact_score REAL DEFAULT 0.0,
                    contract_hex_stamp TEXT,
                    FOREIGN KEY (producer_did) REFERENCES nodes(did)
                );
                
                CREATE INDEX IF NOT EXISTS idx_windows_did ON windows(did);
                CREATE INDEX IF NOT EXISTS idx_windows_time ON windows(start_time, end_time);
                CREATE INDEX IF NOT EXISTS idx_rewards_did ON rewards(did);
                CREATE INDEX IF NOT EXISTS idx_shards_did ON shards(producer_did);
                CREATE INDEX IF NOT EXISTS idx_shards_time ON shards(timestamp);
            """)
    
    def upsert_node(self, profile: NodeProfile) -> None:
        """Insert or update node profile."""
        with self.get_connection() as conn:
            conn.execute("""
                INSERT INTO nodes (did, node_id, total_shards, total_eco_wealth,
                                   current_step_level, consecutive_safe_windows,
                                   last_reward_time, brain_identity_hash, last_active)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(did) DO UPDATE SET
                    node_id = excluded.node_id,
                    total_shards = excluded.total_shards,
                    total_eco_wealth = excluded.total_eco_wealth,
                    current_step_level = excluded.current_step_level,
                    consecutive_safe_windows = excluded.consecutive_safe_windows,
                    last_reward_time = excluded.last_reward_time,
                    brain_identity_hash = excluded.brain_identity_hash,
                    last_active = excluded.last_active
            """, (profile.did, profile.node_id, profile.total_shards,
                  profile.total_eco_wealth, profile.current_step_level,
                  profile.consecutive_safe_windows, profile.last_reward_time,
                  profile.brain_identity_hash, profile.last_active))
    
    def insert_window(self, window: WindowRecord) -> None:
        """Insert window record."""
        with self.get_connection() as conn:
            conn.execute("""
                INSERT OR REPLACE INTO windows
                (window_id, window_type, start_time, end_time, node_id, did,
                 shard_count, avg_knowledge, avg_eco_impact, avg_risk,
                 residual_open, residual_close, is_valid, eco_wealth_minted, step_level)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """, (window.window_id, window.window_type.value, window.start_time,
                  window.end_time, window.node_id, window.did, window.shard_count,
                  window.avg_ker.knowledge, window.avg_ker.eco_impact,
                  window.avg_ker.risk, window.residual_open, window.residual_close,
                  1 if window.is_valid else 0, window.eco_wealth_minted, window.step_level))
    
    def insert_reward(self, reward: RewardEvent) -> None:
        """Insert reward event."""
        with self.get_connection() as conn:
            conn.execute("""
                INSERT INTO rewards
                (event_id, did, window_id, event_type, amount, timestamp, step_level, reason)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """, (reward.event_id, reward.did, reward.window_id, reward.event_type,
                  reward.amount, reward.timestamp, reward.step_level, reward.reason))
    
    def get_node_profile(self, did: str) -> Optional[NodeProfile]:
        """Retrieve node profile by DID."""
        with self.get_connection() as conn:
            row = conn.execute("SELECT * FROM nodes WHERE did = ?", (did,)).fetchone()
            if row:
                return NodeProfile(
                    did=row['did'],
                    node_id=row['node_id'],
                    total_shards=row['total_shards'],
                    total_eco_wealth=row['total_eco_wealth'],
                    current_step_level=row['current_step_level'],
                    consecutive_safe_windows=row['consecutive_safe_windows'],
                    last_reward_time=row['last_reward_time'],
                    brain_identity_hash=row['brain_identity_hash'],
                    created_at=row['created_at'],
                    last_active=row['last_active']
                )
            return None
    
    def get_window_history(self, did: str, limit: int = 100) -> List[WindowRecord]:
        """Retrieve window history for a node."""
        with self.get_connection() as conn:
            rows = conn.execute("""
                SELECT * FROM windows WHERE did = ?
                ORDER BY start_time DESC LIMIT ?
            """, (did, limit)).fetchall()
            
            return [
                WindowRecord(
                    window_id=row['window_id'],
                    window_type=WindowType(row['window_type']),
                    start_time=row['start_time'],
                    end_time=row['end_time'],
                    node_id=row['node_id'],
                    did=row['did'],
                    shard_count=row['shard_count'],
                    avg_ker=KerTriad(
                        knowledge=row['avg_knowledge'],
                        eco_impact=row['avg_eco_impact'],
                        risk=row['avg_risk']
                    ),
                    residual_open=row['residual_open'],
                    residual_close=row['residual_close'],
                    is_valid=bool(row['is_valid']),
                    eco_wealth_minted=row['eco_wealth_minted'],
                    step_level=row['step_level']
                )
                for row in rows
            ]
    
    def get_total_minted(self, did: str) -> float:
        """Get total tokens minted for a node."""
        with self.get_connection() as conn:
            row = conn.execute("""
                SELECT COALESCE(SUM(amount), 0) as total
                FROM rewards WHERE did = ? AND event_type = 'mint'
            """, (did,)).fetchone()
            return row['total'] if row else 0.0
    
    def get_total_burned(self, did: str) -> float:
        """Get total tokens burned for a node."""
        with self.get_connection() as conn:
            row = conn.execute("""
                SELECT COALESCE(SUM(amount), 0) as total
                FROM rewards WHERE did = ? AND event_type = 'burn'
            """, (did,)).fetchone()
            return row['total'] if row else 0.0

# ============================================================================
# Shard Parser & Loader
# ============================================================================

class ShardLoader:
    """Loads and parses ResponseShard CSV files."""
    
    def __init__(self, input_dir: str):
        self.input_dir = Path(input_dir)
        self.logger = logging.getLogger(__name__)
    
    def load_daily_shards(self, date_str: Optional[str] = None) -> List[ResponseShard]:
        """Load shards from daily CSV files."""
        shards = []
        
        if date_str:
            files = [self.input_dir / f"daily_shards_{date_str}.csv"]
        else:
            files = list(self.input_dir.glob("daily_shards_*.csv"))
        
        for file_path in sorted(files):
            if not file_path.exists():
                continue
            
            try:
                with open(file_path, 'r', newline='') as f:
                    reader = csv.DictReader(f)
                    for row in reader:
                        try:
                            shard = ResponseShard.from_csv_row(row)
                            if shard.ker.valid():
                                shards.append(shard)
                        except (KeyError, ValueError) as e:
                            self.logger.warning(f"Skipping malformed row: {e}")
            except Exception as e:
                self.logger.error(f"Failed to load {file_path}: {e}")
        
        self.logger.info(f"Loaded {len(shards)} shards from {len(files)} files")
        return shards
    
    def load_node_shards(self, node_id: str) -> List[ResponseShard]:
        """Load shards for a specific node."""
        file_path = self.input_dir / f"node_{node_id}_shards.csv"
        shards = []
        
        if not file_path.exists():
            return shards
        
        with open(file_path, 'r', newline='') as f:
            reader = csv.DictReader(f)
            for row in reader:
                try:
                    shard = ResponseShard.from_csv_row(row)
                    if shard.node_id == node_id and shard.ker.valid():
                        shards.append(shard)
                except (KeyError, ValueError) as e:
                    self.logger.warning(f"Skipping malformed row: {e}")
        
        return shards

# ============================================================================
# Window Aggregator
# ============================================================================

class WindowAggregator:
    """Aggregates shards into time windows for analysis."""
    
    def __init__(self, config: Config):
        self.config = config
        self.logger = logging.getLogger(__name__)
    
    def create_window_id(self, start_time: int, window_type: WindowType) -> str:
        """Generate unique window identifier."""
        dt = datetime.utcfromtimestamp(start_time)
        if window_type == WindowType.SHORT:
            return f"w_short_{dt.strftime('%Y%m%d_%H%M')}"
        elif window_type == WindowType.DAILY:
            return f"w_daily_{dt.strftime('%Y%m%d')}"
        else:
            return f"w_quarterly_{dt.strftime('%Y%W')}"
    
    def aggregate_shards(self, shards: List[ResponseShard],
                         window_type: WindowType) -> Dict[str, WindowRecord]:
        """Aggregate shards into time windows."""
        window_duration = {
            WindowType.SHORT: self.config.WINDOW_SHORT,
            WindowType.DAILY: self.config.WINDOW_DAILY,
            WindowType.QUARTERLY: self.config.WINDOW_QUARTERLY
        }[window_type]
        
        # Group shards by node and time window
        windows: Dict[str, List[ResponseShard]] = {}
        
        for shard in shards:
            # Calculate window start time
            window_start = (shard.timestamp // window_duration) * window_duration
            key = f"{shard.node_id}_{shard.producer_did}_{window_start}"
            
            if key not in windows:
                windows[key] = []
            windows[key].append(shard)
        
        # Create window records
        records = {}
        for key, shard_list in windows.items():
            if not shard_list:
                continue
            
            node_id = shard_list[0].node_id
            did = shard_list[0].producer_did
            start_time = shard_list[0].timestamp // window_duration * window_duration
            end_time = start_time + window_duration
            
            # Compute averages
            avg_k = statistics.mean([s.ker.knowledge for s in shard_list])
            avg_e = statistics.mean([s.ker.eco_impact for s in shard_list])
            avg_r = statistics.mean([s.ker.risk for s in shard_list])
            
            residual_open = min([s.residual for s in shard_list])
            residual_close = max([s.residual for s in shard_list])
            
            # Validate window
            is_valid = (
                avg_k >= self.config.MIN_KNOWLEDGE_FACTOR and
                avg_e >= self.config.MIN_ECO_IMPACT and
                avg_r <= self.config.MAX_RISK_HARM and
                residual_close <= residual_open  # V_t+1 <= V_t
            )
            
            window_id = self.create_window_id(start_time, window_type)
            
            records[window_id] = WindowRecord(
                window_id=window_id,
                window_type=window_type,
                start_time=start_time,
                end_time=end_time,
                node_id=node_id,
                did=did,
                shard_count=len(shard_list),
                avg_ker=KerTriad(knowledge=avg_k, eco_impact=avg_e, risk=avg_r),
                residual_open=residual_open,
                residual_close=residual_close,
                is_valid=is_valid
            )
        
        self.logger.info(f"Created {len(records)} {window_type.value} windows")
        return records

# ============================================================================
# Eco-Wealth Calculator
# ============================================================================

class EcoWealthCalculator:
    """Calculates token minting and burning based on window performance."""
    
    def __init__(self, config: Config, db: DatabaseManager):
        self.config = config
        self.db = db
        self.logger = logging.getLogger(__name__)
    
    def calculate_step_multiplier(self, step_level: int) -> float:
        """Calculate reward multiplier based on step level."""
        return self.config.STEP_MULTIPLIER_BASE + (
            self.config.STEP_MULTIPLIER_INCREMENT * step_level
        )
    
    def calculate_mint_amount(self, window: WindowRecord,
                               step_level: int) -> float:
        """Calculate token mint amount for a valid window."""
        if not window.is_valid:
            return 0.0
        
        multiplier = self.calculate_step_multiplier(step_level)
        base_amount = self.config.MINT_RATE_BASE
        
        # Weight by eco-impact
        mint_amount = base_amount * multiplier * window.avg_ker.eco_impact
        
        return round(mint_amount, 6)
    
    def calculate_burn_amount(self, window: WindowRecord,
                               consecutive_failures: int) -> float:
        """Calculate token burn amount for regression."""
        if consecutive_failures < self.config.BURN_REGRESSION_WINDOWS:
            return 0.0
        
        # Burn proportional to regression severity
        burn_rate = 0.1 * (consecutive_failures - self.config.BURN_REGRESSION_WINDOWS + 1)
        burn_amount = self.config.MINT_RATE_BASE * burn_rate
        
        return round(min(burn_amount, self.config.MINT_RATE_BASE), 6)
    
    def process_window(self, window: WindowRecord,
                       node_profile: NodeProfile) -> Optional[RewardEvent]:
        """Process a window and generate reward event."""
        # Check if window qualifies for minting
        if window.is_valid:
            mint_amount = self.calculate_mint_amount(window, node_profile.current_step_level)
            
            if mint_amount > 0:
                event_id = hashlib.sha256(
                    f"mint_{window.window_id}_{window.did}".encode()
                ).hexdigest()[:16]
                
                return RewardEvent(
                    event_id=event_id,
                    did=window.did,
                    window_id=window.window_id,
                    event_type="mint",
                    amount=mint_amount,
                    timestamp=window.end_time,
                    step_level=node_profile.current_step_level,
                    reason=f"Valid window with E={window.avg_ker.eco_impact:.4f}"
                )
        
        # Check for burn conditions (eco-impact regression)
        elif window.avg_ker.eco_impact < self.config.MIN_ECO_IMPACT:
            burn_amount = self.calculate_burn_amount(
                window, node_profile.consecutive_safe_windows
            )
            
            if burn_amount > 0:
                event_id = hashlib.sha256(
                    f"burn_{window.window_id}_{window.did}".encode()
                ).hexdigest()[:16]
                
                return RewardEvent(
                    event_id=event_id,
                    did=window.did,
                    window_id=window.window_id,
                    event_type="burn",
                    amount=burn_amount,
                    timestamp=window.end_time,
                    step_level=node_profile.current_step_level,
                    reason=f"Eco-impact regression for {node_profile.consecutive_safe_windows} windows"
                )
        
        return None
    
    def update_node_profile(self, profile: NodeProfile,
                            window: WindowRecord,
                            reward: Optional[RewardEvent]) -> NodeProfile:
        """Update node profile based on window results."""
        profile.last_active = window.end_time
        
        if window.is_valid:
            profile.consecutive_safe_windows += 1
            
            # Check for step advancement
            if (profile.consecutive_safe_windows >=
                    self.config.STEP_WINDOW_REQUIREMENT):
                profile.current_step_level += 1
                profile.consecutive_safe_windows = 0
                self.logger.info(f"Step up granted: {profile.did} -> Level {profile.current_step_level}")
        else:
            profile.consecutive_safe_windows = 0
        
        if reward:
            if reward.event_type == "mint":
                profile.total_eco_wealth += reward.amount
            elif reward.event_type == "burn":
                profile.total_eco_wealth = max(0.0, profile.total_eco_wealth - reward.amount)
            
            profile.last_reward_time = reward.timestamp
        
        return profile

# ============================================================================
# Climbing-Steps Manager
# ============================================================================

class ClimbingStepsManager:
    """Manages progressive step advancement for nodes."""
    
    def __init__(self, config: Config, db: DatabaseManager):
        self.config = config
        self.db = db
        self.logger = logging.getLogger(__name__)
    
    def check_step_eligibility(self, did: str) -> Tuple[bool, str]:
        """Check if a node is eligible for step advancement."""
        profile = self.db.get_node_profile(did)
        if not profile:
            return False, "Node profile not found"
        
        if profile.consecutive_safe_windows < self.config.STEP_WINDOW_REQUIREMENT:
            remaining = self.config.STEP_WINDOW_REQUIREMENT - profile.consecutive_safe_windows
            return False, f"Need {remaining} more consecutive safe windows"
        
        # Verify residual risk trend
        windows = self.db.get_window_history(did, self.config.STEP_WINDOW_REQUIREMENT)
        if len(windows) < self.config.STEP_WINDOW_REQUIREMENT:
            return False, "Insufficient window history"
        
        # Check V_t non-increasing across the run
        for i in range(1, len(windows)):
            if windows[i].residual_close > windows[i-1].residual_close:
                return False, "Residual risk increased in recent windows"
        
        return True, "Eligible for step advancement"
    
    def request_step_up(self, did: str) -> Tuple[bool, int, str]:
        """Request step advancement for a node."""
        eligible, reason = self.check_step_eligibility(did)
        
        if not eligible:
            return False, 0, reason
        
        profile = self.db.get_node_profile(did)
        if not profile:
            return False, 0, "Node profile not found"
        
        new_level = profile.current_step_level + 1
        profile.current_step_level = new_level
        profile.consecutive_safe_windows = 0
        self.db.upsert_node(profile)
        
        self.logger.info(f"Step up granted: {did} -> Level {new_level}")
        return True, new_level, f"Step advancement to level {new_level}"

# ============================================================================
# Analytics Dashboard Generator
# ============================================================================

class DashboardGenerator:
    """Generates analytics reports and dashboards."""
    
    def __init__(self, config: Config, db: DatabaseManager):
        self.config = config
        self.db = db
        self.output_dir = Path(config.ANALYSIS_OUTPUT_DIR)
        self.output_dir.mkdir(parents=True, exist_ok=True)
        self.logger = logging.getLogger(__name__)
    
    def generate_node_report(self, did: str) -> Dict[str, Any]:
        """Generate comprehensive report for a node."""
        profile = self.db.get_node_profile(did)
        if not profile:
            return {"error": "Node not found"}
        
        windows = self.db.get_window_history(did, 100)
        total_minted = self.db.get_total_minted(did)
        total_burned = self.db.get_total_burned(did)
        
        # Calculate trends
        if len(windows) >= 2:
            recent_e = [w.avg_ker.eco_impact for w in windows[:10]]
            e_trend = "increasing" if recent_e[0] > recent_e[-1] else "decreasing"
            
            recent_r = [w.avg_ker.risk for w in windows[:10]]
            r_trend = "decreasing" if recent_r[0] < recent_r[-1] else "increasing"
        else:
            e_trend = "insufficient_data"
            r_trend = "insufficient_data"
        
        report = {
            "did": did,
            "node_id": profile.node_id,
            "current_step_level": profile.current_step_level,
            "total_shards": profile.total_shards,
            "total_eco_wealth": round(profile.total_eco_wealth, 6),
            "total_minted": round(total_minted, 6),
            "total_burned": round(total_burned, 6),
            "consecutive_safe_windows": profile.consecutive_safe_windows,
            "brain_identity_linked": profile.brain_identity_hash is not None,
            "eco_impact_trend": e_trend,
            "risk_trend": r_trend,
            "last_active": datetime.utcfromtimestamp(profile.last_active).isoformat(),
            "contract_hex_stamp": self.config.CONTRACT_HEX_STAMP,
            "generated_at": datetime.utcnow().isoformat()
        }
        
        return report
    
    def generate_system_summary(self) -> Dict[str, Any]:
        """Generate system-wide summary report."""
        # This would query all nodes in production
        summary = {
            "total_nodes": 0,
            "total_shards_processed": 0,
            "total_eco_wealth_in_circulation": 0.0,
            "average_step_level": 0.0,
            "system_residual_risk": 0.0,
            "contract_version": self.config.CONTRACT_VERSION,
            "contract_hex_stamp": self.config.CONTRACT_HEX_STAMP,
            "generated_at": datetime.utcnow().isoformat()
        }
        
        return summary
    
    def save_report(self, report: Dict[str, Any], filename: str) -> Path:
        """Save report to JSON file."""
        filepath = self.output_dir / filename
        with open(filepath, 'w') as f:
            json.dump(report, f, indent=2, default=str)
        
        self.logger.info(f"Report saved: {filepath}")
        return filepath
    
    def export_csv_summary(self, did: str) -> Path:
        """Export window history to CSV."""
        windows = self.db.get_window_history(did, 1000)
        filepath = self.output_dir / f"window_history_{did.replace(':', '_')}.csv"
        
        with open(filepath, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerow([
                'window_id', 'window_type', 'start_time', 'end_time',
                'shard_count', 'avg_knowledge', 'avg_eco_impact', 'avg_risk',
                'residual_open', 'residual_close', 'is_valid', 'eco_wealth_minted', 'step_level'
            ])
            
            for w in windows:
                writer.writerow([
                    w.window_id, w.window_type.value, w.start_time, w.end_time,
                    w.shard_count, w.avg_ker.knowledge, w.avg_ker.eco_impact, w.avg_ker.risk,
                    w.residual_open, w.residual_close, 1 if w.is_valid else 0,
                    w.eco_wealth_minted, w.step_level
                ])
        
        self.logger.info(f"CSV export saved: {filepath}")
        return filepath

# ============================================================================
# Main Tracker Class
# ============================================================================

class EcoWealthTracker:
    """Main orchestrator for eco-wealth tracking and analysis."""
    
    def __init__(self, config: Optional[Config] = None):
        self.config = config or Config()
        
        # Setup logging
        logging.basicConfig(
            level=logging.INFO,
            format='%(asctime)s [%(levelname)s] %(name)s: %(message)s'
        )
        self.logger = logging.getLogger(__name__)
        
        # Initialize components
        self.db = DatabaseManager(self.config.DATABASE_PATH)
        self.shard_loader = ShardLoader(self.config.SHARD_INPUT_DIR)
        self.window_aggregator = WindowAggregator(self.config)
        self.wealth_calculator = EcoWealthCalculator(self.config, self.db)
        self.steps_manager = ClimbingStepsManager(self.config, self.db)
        self.dashboard = DashboardGenerator(self.config, self.db)
        
        self.logger.info(f"EcoWealthTracker initialized (Contract: {self.config.CONTRACT_HEX_STAMP})")
    
    def process_daily_shards(self, date_str: Optional[str] = None) -> int:
        """Process daily shards and update eco-wealth records."""
        self.logger.info("Starting daily shard processing...")
        
        # Load shards
        shards = self.shard_loader.load_daily_shards(date_str)
        if not shards:
            self.logger.warning("No shards to process")
            return 0
        
        # Aggregate into daily windows
        windows = self.window_aggregator.aggregate_shards(shards, WindowType.DAILY)
        
        # Process each window
        rewards_issued = 0
        for window_id, window in windows.items():
            # Get or create node profile
            profile = self.db.get_node_profile(window.did)
            if not profile:
                profile = NodeProfile(
                    did=window.did,
                    node_id=window.node_id,
                    created_at=window.start_time
                )
            
            # Calculate reward
            reward = self.wealth_calculator.process_window(window, profile)
            
            # Update profile
            profile = self.wealth_calculator.update_node_profile(profile, window, reward)
            
            # Persist to database
            self.db.upsert_node(profile)
            self.db.insert_window(window)
            
            if reward:
                self.db.insert_reward(reward)
                rewards_issued += 1
                self.logger.info(f"Reward issued: {reward.event_type} {reward.amount} to {window.did}")
        
        self.logger.info(f"Processing complete: {len(windows)} windows, {rewards_issued} rewards")
        return rewards_issued
    
    def generate_reports(self, did: Optional[str] = None) -> List[Path]:
        """Generate analytics reports."""
        reports = []
        
        if did:
            report = self.dashboard.generate_node_report(did)
            filepath = self.dashboard.save_report(report, f"node_report_{did.replace(':', '_')}.json")
            reports.append(filepath)
            
            csv_path = self.dashboard.export_csv_summary(did)
            reports.append(csv_path)
        else:
            summary = self.dashboard.generate_system_summary()
            filepath = self.dashboard.save_report(summary, "system_summary.json")
            reports.append(filepath)
        
        return reports
    
    def check_all_nodes_step_eligibility(self) -> Dict[str, Tuple[bool, str]]:
        """Check step eligibility for all tracked nodes."""
        # In production, query all DIDs from database
        results = {}
        # Placeholder for iteration logic
        return results

# ============================================================================
# Command-Line Interface
# ============================================================================

def main():
    """CLI entry point."""
    import argparse
    
    parser = argparse.ArgumentParser(description="Ecotribute Eco-Wealth Tracker")
    parser.add_argument("--config", type=str, help="Path to config file")
    parser.add_argument("--process", action="store_true", help="Process daily shards")
    parser.add_argument("--date", type=str, help="Date string (YYYYMMDD) for processing")
    parser.add_argument("--report", type=str, help="Generate report for DID")
    parser.add_argument("--summary", action="store_true", help="Generate system summary")
    parser.add_argument("--step-check", type=str, help="Check step eligibility for DID")
    
    args = parser.parse_args()
    
    tracker = EcoWealthTracker()
    
    if args.process:
        count = tracker.process_daily_shards(args.date)
        print(f"Processed {count} reward events")
    
    if args.report:
        paths = tracker.generate_reports(args.report)
        print(f"Reports generated: {paths}")
    
    if args.summary:
        paths = tracker.generate_reports()
        print(f"Summary generated: {paths}")
    
    if args.step_check:
        eligible, level, reason = tracker.steps_manager.request_step_up(args.step_check)
        print(f"Step check: {reason}")
        if eligible:
            print(f"New step level: {level}")

if __name__ == "__main__":
    main()

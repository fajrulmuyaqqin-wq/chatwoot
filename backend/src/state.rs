use dashmap::DashMap;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

use crate::config::Config;

pub type AccountId = i32;
pub type UserId = i32;

#[allow(dead_code)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CableBroadcastMsg {
    pub account_id: AccountId,
    pub event: String,
    pub data: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct TenantTokenBucket {
    pub tokens: f64,
    pub last_refill: Instant,
}

impl TenantTokenBucket {
    pub fn new(capacity: f64) -> Self {
        Self {
            tokens: capacity,
            last_refill: Instant::now(),
        }
    }

    /// Tries to acquire `cost` tokens using standard token-bucket algorithm.
    pub fn try_acquire(&mut self, cost: f64, capacity: f64, refill_rate: f64) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * refill_rate).min(capacity);
        self.last_refill = now;

        if self.tokens >= cost {
            self.tokens -= cost;
            true
        } else {
            false
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub presence: Arc<DashMap<AccountId, DashMap<UserId, Instant>>>,
    pub webhook_rate_limiter: Arc<DashMap<AccountId, TenantTokenBucket>>,
    pub broadcast_tx: broadcast::Sender<CableBroadcastMsg>,
}

impl AppState {
    pub fn new(db: PgPool, config: Config) -> Self {
        let (broadcast_tx, _broadcast_rx) = broadcast::channel(1024);
        Self {
            db,
            config: Arc::new(config),
            presence: Arc::new(DashMap::new()),
            webhook_rate_limiter: Arc::new(DashMap::new()),
            broadcast_tx,
        }
    }

    /// Token-bucket rate limiter per tenant (account_id) for incoming webhook events.
    /// Default: 120 burst tokens, refills at 30 tokens/second per tenant.
    pub fn check_tenant_webhook_rate_limit(&self, account_id: AccountId) -> bool {
        let capacity = 120.0;
        let refill_rate = 30.0;
        let mut entry = self
            .webhook_rate_limiter
            .entry(account_id)
            .or_insert_with(|| TenantTokenBucket::new(capacity));
        entry.try_acquire(1.0, capacity, refill_rate)
    }
}

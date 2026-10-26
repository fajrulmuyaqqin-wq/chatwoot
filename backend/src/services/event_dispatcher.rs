use crate::state::{AccountId, AppState, CableBroadcastMsg};

/// Mem-broadcast event ke seluruh WebSocket client yang terhubung ke akun ini via ActionCable
pub fn dispatch_event(
    state: &AppState,
    account_id: AccountId,
    event_name: &str,
    data: serde_json::Value,
) {
    let _ = state.broadcast_tx.send(CableBroadcastMsg {
        account_id,
        event: event_name.to_string(),
        data,
    });
}

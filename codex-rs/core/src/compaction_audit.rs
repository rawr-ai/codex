use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;

use crate::protocol::CompactionTrigger;
use codex_protocol::ThreadId;

static NEXT_COMPACTION_TRIGGER: OnceLock<Mutex<HashMap<ThreadId, CompactionTrigger>>> =
    OnceLock::new();

fn next_compaction_trigger() -> &'static Mutex<HashMap<ThreadId, CompactionTrigger>> {
    NEXT_COMPACTION_TRIGGER.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Set metadata describing why the *next* compaction should be attributed.
///
/// This is primarily used by the rawr auto-compaction watcher in the TUI to
/// record `auto_watcher` as the trigger on the resulting `RolloutItem::Compacted`.
pub fn set_next_compaction_trigger(thread_id: ThreadId, trigger: CompactionTrigger) {
    next_compaction_trigger()
        .lock()
        .unwrap_or_else(|_| panic!("compaction audit mutex poisoned"))
        .insert(thread_id, trigger);
}

/// Peek (without clearing) any pending compaction trigger.
pub(crate) fn peek_next_compaction_trigger(thread_id: ThreadId) -> Option<CompactionTrigger> {
    next_compaction_trigger()
        .lock()
        .unwrap_or_else(|_| panic!("compaction audit mutex poisoned"))
        .get(&thread_id)
        .cloned()
}

/// Take and clear any pending compaction trigger.
pub(crate) fn take_next_compaction_trigger(thread_id: ThreadId) -> Option<CompactionTrigger> {
    next_compaction_trigger()
        .lock()
        .unwrap_or_else(|_| panic!("compaction audit mutex poisoned"))
        .remove(&thread_id)
}

/// Clear any pending compaction trigger without consuming it as a compaction.
pub fn clear_next_compaction_trigger(thread_id: ThreadId) {
    next_compaction_trigger()
        .lock()
        .unwrap_or_else(|_| panic!("compaction audit mutex poisoned"))
        .remove(&thread_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::protocol::CompactionPacketAuthor;
    use pretty_assertions::assert_eq;

    #[test]
    fn clear_removes_pending_trigger() {
        let thread_id = ThreadId::new();
        let trigger = CompactionTrigger::AutoWatcher {
            trigger_percent_remaining: 42,
            saw_commit: true,
            saw_plan_checkpoint: false,
            saw_plan_update: false,
            saw_pr_checkpoint: false,
            packet_author: CompactionPacketAuthor::Watcher,
        };
        set_next_compaction_trigger(thread_id, trigger.clone());
        assert_eq!(peek_next_compaction_trigger(thread_id), Some(trigger));
        clear_next_compaction_trigger(thread_id);
        assert_eq!(take_next_compaction_trigger(thread_id), None);
    }
}

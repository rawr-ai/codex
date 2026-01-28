use std::sync::Mutex;
use std::sync::OnceLock;

use crate::protocol::CompactionTrigger;

static NEXT_COMPACTION_TRIGGER: OnceLock<Mutex<Option<CompactionTrigger>>> = OnceLock::new();

fn next_compaction_trigger() -> &'static Mutex<Option<CompactionTrigger>> {
    NEXT_COMPACTION_TRIGGER.get_or_init(|| Mutex::new(None))
}

/// Set metadata describing why the *next* compaction should be attributed.
///
/// This is primarily used by the rawr auto-compaction watcher in the TUI to
/// record `auto_watcher` as the trigger on the resulting `RolloutItem::Compacted`.
pub fn set_next_compaction_trigger(trigger: CompactionTrigger) {
    *next_compaction_trigger()
        .lock()
        .expect("compaction audit mutex poisoned") = Some(trigger);
}

/// Take and clear any pending compaction trigger.
pub(crate) fn take_next_compaction_trigger() -> Option<CompactionTrigger> {
    next_compaction_trigger()
        .lock()
        .expect("compaction audit mutex poisoned")
        .take()
}

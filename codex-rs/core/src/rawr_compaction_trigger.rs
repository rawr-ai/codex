use crate::config::types::RawrAutoCompactionPacketAuthor;
use crate::protocol::CompactionPacketAuthor;
use crate::protocol::CompactionTrigger;

pub fn packet_author_from_rawr_config(
    packet_author: RawrAutoCompactionPacketAuthor,
) -> CompactionPacketAuthor {
    match packet_author {
        RawrAutoCompactionPacketAuthor::Watcher => CompactionPacketAuthor::Watcher,
        RawrAutoCompactionPacketAuthor::Agent => CompactionPacketAuthor::Agent,
    }
}

pub fn auto_watcher_trigger(
    trigger_percent_remaining: i64,
    saw_commit: bool,
    saw_plan_checkpoint: bool,
    saw_plan_update: bool,
    saw_pr_checkpoint: bool,
    packet_author: CompactionPacketAuthor,
) -> CompactionTrigger {
    CompactionTrigger::AutoWatcher {
        trigger_percent_remaining,
        saw_commit,
        saw_plan_checkpoint,
        saw_plan_update,
        saw_pr_checkpoint,
        packet_author,
    }
}

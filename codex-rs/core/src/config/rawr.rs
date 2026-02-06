use crate::config::Config;
use crate::config::types::RawrAutoCompactionPacketAuthor;

pub fn packet_author(config: &Config) -> RawrAutoCompactionPacketAuthor {
    config
        .rawr_auto_compaction
        .as_ref()
        .and_then(|rawr| rawr.packet_author)
        .unwrap_or(RawrAutoCompactionPacketAuthor::Watcher)
}

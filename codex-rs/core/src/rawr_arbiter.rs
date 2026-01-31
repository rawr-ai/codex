use crate::config::Config;
use crate::config::types::RawrAutoCompactionBoundary;
use crate::rawr_auto_compaction::RawrAutoCompactionSignals;
use crate::rawr_auto_compaction::RawrAutoCompactionThresholds;
use crate::rawr_auto_compaction::rawr_pick_tier;
use crate::rawr_auto_compaction::rawr_should_compact_mid_turn;
use crate::rawr_structured_state::RawrBoundaryEvent;
use crate::rawr_structured_state::RawrBoundaryKind;
use crate::rawr_structured_state::RawrCompactionDecision;
use crate::rawr_structured_state::RawrDecisionAction;
use crate::rawr_structured_state::RawrDecisionReason;
use crate::rawr_structured_state::new_compaction_decision;
use crate::rawr_structured_state::new_token_pressure_decision;

#[derive(Debug, Clone, Copy)]
pub(crate) struct RawrTokenContext {
    pub total_usage_tokens: i64,
    pub model_context_window: Option<i64>,
}

impl RawrTokenContext {
    fn percent_remaining(self) -> Option<i64> {
        let context_window = self.model_context_window?;
        if context_window <= 0 {
            return Some(0);
        }
        let remaining = context_window.saturating_sub(self.total_usage_tokens);
        Some(remaining.saturating_mul(100) / context_window)
    }
}

pub(crate) struct RawrArbiter;

impl RawrArbiter {
    pub(crate) fn evaluate_boundary_event(
        config: &Config,
        state: &crate::rawr_structured_state::RawrStructuredState,
        event: &RawrBoundaryEvent,
        decision_seq: u64,
        token: RawrTokenContext,
    ) -> RawrCompactionDecision {
        let mut decision = new_compaction_decision(event, decision_seq, token.total_usage_tokens);
        decision.model_context_window = token.model_context_window;
        decision.turn_signals = state.current_turn.clone();

        let Some(percent_remaining) = token.percent_remaining() else {
            decision
                .reasons
                .push(RawrDecisionReason::MissingContextWindow);
            return decision;
        };
        decision.percent_remaining = Some(percent_remaining);

        let Some(tier) = rawr_pick_tier(
            RawrAutoCompactionThresholds::from_config(config),
            percent_remaining,
        ) else {
            decision.reasons.push(RawrDecisionReason::AboveThreshold);
            return decision;
        };
        decision.tier = Some(
            match tier {
                crate::rawr_auto_compaction::RawrAutoCompactionTier::Early => "early",
                crate::rawr_auto_compaction::RawrAutoCompactionTier::Ready => "ready",
                crate::rawr_auto_compaction::RawrAutoCompactionTier::Asap => "asap",
                crate::rawr_auto_compaction::RawrAutoCompactionTier::Emergency => "emergency",
            }
            .to_string(),
        );

        let mut signals = RawrAutoCompactionSignals::default();
        if let Some(turn_signals) = state.current_turn.as_ref()
            && turn_signals.turn_id == event.turn_id
        {
            signals.saw_commit = turn_signals.saw_commit;
            signals.saw_plan_checkpoint = turn_signals.saw_plan_checkpoint;
            signals.saw_plan_update = turn_signals.saw_plan_update;
            signals.saw_pr_checkpoint = turn_signals.saw_pr_checkpoint;
            signals.saw_agent_done = turn_signals.saw_agent_done;
            signals.saw_topic_shift = turn_signals.saw_topic_shift;
            signals.saw_concluding_thought = turn_signals.saw_concluding_thought;
        }

        let boundaries_required: &[RawrAutoCompactionBoundary] = config
            .rawr_auto_compaction
            .as_ref()
            .and_then(|rawr| rawr.trigger.as_ref())
            .and_then(|trigger| trigger.auto_requires_any_boundary.as_deref())
            .unwrap_or(&[]);

        if rawr_should_compact_mid_turn(config, percent_remaining, &signals, boundaries_required) {
            decision.action = RawrDecisionAction::ConsiderCompaction;
            decision.reasons.push(RawrDecisionReason::EligibleByPolicy);
        } else {
            decision
                .reasons
                .push(RawrDecisionReason::BoundaryGatingNotSatisfied);
        }

        decision
    }

    pub(crate) fn evaluate_token_pressure_mid_turn(
        config: &Config,
        thread_id: codex_protocol::ThreadId,
        turn_id: &str,
        signals: &RawrAutoCompactionSignals,
        decision_seq: u64,
        token: RawrTokenContext,
    ) -> RawrCompactionDecision {
        let mut decision =
            new_token_pressure_decision(thread_id, turn_id, decision_seq, token.total_usage_tokens);
        decision.model_context_window = token.model_context_window;

        decision.turn_signals = Some(crate::rawr_structured_state::RawrTurnSignals {
            turn_id: turn_id.to_string(),
            saw_plan_update: signals.saw_plan_update,
            saw_plan_checkpoint: signals.saw_plan_checkpoint,
            saw_commit: signals.saw_commit,
            saw_pr_checkpoint: signals.saw_pr_checkpoint,
            saw_agent_done: signals.saw_agent_done,
            saw_topic_shift: signals.saw_topic_shift,
            saw_concluding_thought: signals.saw_concluding_thought,
        });

        let Some(percent_remaining) = token.percent_remaining() else {
            decision
                .reasons
                .push(RawrDecisionReason::MissingContextWindow);
            return decision;
        };
        decision.percent_remaining = Some(percent_remaining);

        let Some(tier) = rawr_pick_tier(
            RawrAutoCompactionThresholds::from_config(config),
            percent_remaining,
        ) else {
            decision.reasons.push(RawrDecisionReason::AboveThreshold);
            return decision;
        };
        decision.tier = Some(
            match tier {
                crate::rawr_auto_compaction::RawrAutoCompactionTier::Early => "early",
                crate::rawr_auto_compaction::RawrAutoCompactionTier::Ready => "ready",
                crate::rawr_auto_compaction::RawrAutoCompactionTier::Asap => "asap",
                crate::rawr_auto_compaction::RawrAutoCompactionTier::Emergency => "emergency",
            }
            .to_string(),
        );

        let boundaries_required: &[RawrAutoCompactionBoundary] = config
            .rawr_auto_compaction
            .as_ref()
            .and_then(|rawr| rawr.trigger.as_ref())
            .and_then(|trigger| trigger.auto_requires_any_boundary.as_deref())
            .unwrap_or(&[]);

        if rawr_should_compact_mid_turn(config, percent_remaining, signals, boundaries_required) {
            decision.action = RawrDecisionAction::ConsiderCompaction;
            decision.reasons.push(RawrDecisionReason::EligibleByPolicy);
        } else {
            decision
                .reasons
                .push(RawrDecisionReason::BoundaryGatingNotSatisfied);
        }

        decision
    }
}

pub(crate) fn should_persist_shadow_decision(
    event_kind: &RawrBoundaryKind,
    decision: &RawrCompactionDecision,
) -> bool {
    if matches!(event_kind, RawrBoundaryKind::CompactionCompleted { .. }) {
        return true;
    }
    if decision.action != RawrDecisionAction::NoAction {
        return true;
    }
    decision.tier.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::Feature;
    use crate::rawr_structured_state::RawrBoundaryKind;
    use crate::rawr_structured_state::RawrBoundarySource;
    use crate::rawr_structured_state::RawrDecisionAction;
    use crate::rawr_structured_state::RawrDecisionReason;
    use crate::rawr_structured_state::RawrStructuredState;
    use crate::rawr_structured_state::new_boundary_event;
    use codex_protocol::ThreadId;
    use pretty_assertions::assert_eq;

    fn fixed_thread_id() -> ThreadId {
        ThreadId::try_from("00000000-0000-0000-0000-000000000000").expect("valid uuid")
    }

    #[test]
    fn arbiter_suggests_compaction_when_token_pressure_and_boundary_present() {
        let mut config = crate::config::test_config();
        config.features.enable(Feature::RawrAutoCompaction);
        config.model_context_window = Some(1_000);

        let thread_id = fixed_thread_id();
        let mut state = RawrStructuredState::new(thread_id);

        let started = new_boundary_event(
            thread_id,
            "turn-1",
            1,
            RawrBoundarySource::Core,
            RawrBoundaryKind::TurnStarted,
        );
        state.reduce_boundary_event(&started);

        let commit = new_boundary_event(
            thread_id,
            "turn-1",
            2,
            RawrBoundarySource::Tool,
            RawrBoundaryKind::Commit,
        );
        state.reduce_boundary_event(&commit);

        let decision = RawrArbiter::evaluate_boundary_event(
            &config,
            &state,
            &commit,
            1,
            RawrTokenContext {
                total_usage_tokens: 500,
                model_context_window: config.model_context_window,
            },
        );

        assert_eq!(decision.action, RawrDecisionAction::ConsiderCompaction);
        assert_eq!(decision.tier.as_deref(), Some("asap"));
        assert_eq!(decision.reasons, vec![RawrDecisionReason::EligibleByPolicy]);
    }

    #[test]
    fn arbiter_records_missing_context_window() {
        let mut config = crate::config::test_config();
        config.features.enable(Feature::RawrAutoCompaction);

        let thread_id = fixed_thread_id();
        let state = RawrStructuredState::new(thread_id);
        let event = new_boundary_event(
            thread_id,
            "turn-1",
            1,
            RawrBoundarySource::Core,
            RawrBoundaryKind::TurnStarted,
        );

        let decision = RawrArbiter::evaluate_boundary_event(
            &config,
            &state,
            &event,
            1,
            RawrTokenContext {
                total_usage_tokens: 500,
                model_context_window: None,
            },
        );

        assert_eq!(decision.action, RawrDecisionAction::NoAction);
        assert_eq!(
            decision.reasons,
            vec![RawrDecisionReason::MissingContextWindow]
        );
    }

    #[test]
    fn arbiter_records_above_threshold() {
        let mut config = crate::config::test_config();
        config.features.enable(Feature::RawrAutoCompaction);
        config.model_context_window = Some(1_000);

        let thread_id = fixed_thread_id();
        let state = RawrStructuredState::new(thread_id);
        let event = new_boundary_event(
            thread_id,
            "turn-1",
            1,
            RawrBoundarySource::Core,
            RawrBoundaryKind::TurnStarted,
        );

        let decision = RawrArbiter::evaluate_boundary_event(
            &config,
            &state,
            &event,
            1,
            RawrTokenContext {
                total_usage_tokens: 10,
                model_context_window: config.model_context_window,
            },
        );

        assert_eq!(decision.action, RawrDecisionAction::NoAction);
        assert_eq!(decision.reasons, vec![RawrDecisionReason::AboveThreshold]);
    }

    #[test]
    fn arbiter_records_boundary_gating_not_satisfied() {
        let mut config = crate::config::test_config();
        config.features.enable(Feature::RawrAutoCompaction);
        config.model_context_window = Some(1_000);

        let thread_id = fixed_thread_id();
        let mut state = RawrStructuredState::new(thread_id);
        let started = new_boundary_event(
            thread_id,
            "turn-1",
            1,
            RawrBoundarySource::Core,
            RawrBoundaryKind::TurnStarted,
        );
        state.reduce_boundary_event(&started);

        let event = started;
        let decision = RawrArbiter::evaluate_boundary_event(
            &config,
            &state,
            &event,
            1,
            RawrTokenContext {
                total_usage_tokens: 500,
                model_context_window: config.model_context_window,
            },
        );

        assert_eq!(decision.action, RawrDecisionAction::NoAction);
        assert_eq!(
            decision.reasons,
            vec![RawrDecisionReason::BoundaryGatingNotSatisfied]
        );
    }

    #[test]
    fn token_pressure_mid_turn_logs_trappy_early_plan_update_without_semantic_break() {
        let mut config = crate::config::test_config();
        config.features.enable(Feature::RawrAutoCompaction);
        config.model_context_window = Some(1_000);

        let thread_id = fixed_thread_id();
        let mut signals = RawrAutoCompactionSignals::default();
        signals.saw_plan_update = true;

        let decision = RawrArbiter::evaluate_token_pressure_mid_turn(
            &config,
            thread_id,
            "turn-1",
            &signals,
            1,
            RawrTokenContext {
                total_usage_tokens: 200, // 80% remaining -> Early tier
                model_context_window: config.model_context_window,
            },
        );

        assert_eq!(decision.action, RawrDecisionAction::NoAction);
        assert_eq!(decision.tier.as_deref(), Some("early"));
        assert_eq!(
            decision.reasons,
            vec![RawrDecisionReason::BoundaryGatingNotSatisfied]
        );
    }

    #[test]
    fn token_pressure_mid_turn_logs_golden_early_plan_update_with_semantic_break() {
        let mut config = crate::config::test_config();
        config.features.enable(Feature::RawrAutoCompaction);
        config.model_context_window = Some(1_000);

        let thread_id = fixed_thread_id();
        let mut signals = RawrAutoCompactionSignals::default();
        signals.saw_plan_update = true;
        signals.saw_topic_shift = true;

        let decision = RawrArbiter::evaluate_token_pressure_mid_turn(
            &config,
            thread_id,
            "turn-1",
            &signals,
            1,
            RawrTokenContext {
                total_usage_tokens: 200, // 80% remaining -> Early tier
                model_context_window: config.model_context_window,
            },
        );

        assert_eq!(decision.action, RawrDecisionAction::ConsiderCompaction);
        assert_eq!(decision.tier.as_deref(), Some("early"));
        assert_eq!(decision.reasons, vec![RawrDecisionReason::EligibleByPolicy]);
    }

    #[test]
    fn token_pressure_mid_turn_logs_emergency_ignores_boundary_gating() {
        let mut config = crate::config::test_config();
        config.features.enable(Feature::RawrAutoCompaction);
        config.model_context_window = Some(1_000);

        let thread_id = fixed_thread_id();
        let signals = RawrAutoCompactionSignals::default();

        let decision = RawrArbiter::evaluate_token_pressure_mid_turn(
            &config,
            thread_id,
            "turn-1",
            &signals,
            1,
            RawrTokenContext {
                total_usage_tokens: 990, // 1% remaining -> Emergency tier
                model_context_window: config.model_context_window,
            },
        );

        assert_eq!(decision.action, RawrDecisionAction::ConsiderCompaction);
        assert_eq!(decision.tier.as_deref(), Some("emergency"));
        assert_eq!(decision.reasons, vec![RawrDecisionReason::EligibleByPolicy]);
    }

    #[test]
    fn should_persist_shadow_decision_is_true_when_under_token_pressure() {
        let thread_id = fixed_thread_id();
        let event = new_boundary_event(
            thread_id,
            "turn-1",
            1,
            RawrBoundarySource::Core,
            RawrBoundaryKind::TurnStarted,
        );

        let mut decision = new_compaction_decision(&event, 1, 900);
        decision.tier = Some("asap".to_string());
        decision.action = RawrDecisionAction::NoAction;

        assert_eq!(should_persist_shadow_decision(&event.kind, &decision), true);
    }

    #[test]
    fn should_persist_shadow_decision_is_true_when_compaction_completed() {
        let thread_id = fixed_thread_id();
        let event = new_boundary_event(
            thread_id,
            "turn-1",
            1,
            RawrBoundarySource::Compaction,
            RawrBoundaryKind::CompactionCompleted {
                trigger: None,
                total_tokens_before: 100,
                total_tokens_after: 10,
            },
        );

        let decision = new_compaction_decision(&event, 1, 10);
        assert_eq!(should_persist_shadow_decision(&event.kind, &decision), true);
    }

    #[test]
    fn should_persist_shadow_decision_is_false_when_not_under_token_pressure() {
        let thread_id = fixed_thread_id();
        let event = new_boundary_event(
            thread_id,
            "turn-1",
            1,
            RawrBoundarySource::Core,
            RawrBoundaryKind::TurnStarted,
        );

        let decision = new_compaction_decision(&event, 1, 10);
        assert_eq!(
            should_persist_shadow_decision(&event.kind, &decision),
            false
        );
    }
}

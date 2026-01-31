use codex_core::protocol::EventMsg;
use codex_core::protocol::Op;
use pretty_assertions::assert_eq;
use tokio::time::Duration;
use tokio::time::timeout;

use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed_with_tokens;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::test_codex::test_codex;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rawr_auto_compaction_judgment_op_emits_result_without_transcript_events() {
    let server = start_mock_server().await;

    let sse = sse(vec![
        ev_response_created("resp-1"),
        ev_assistant_message(
            "msg-1",
            r#"{"should_compact":false,"reason":"keep context together"}"#,
        ),
        ev_completed_with_tokens("resp-1", 10),
    ]);
    let mock = mount_sse_once(&server, sse).await;

    let mut builder = test_codex();
    let test = builder.build(&server).await.unwrap();

    let prompt_path = test.workspace_path("rawr/prompts/judgment.md");
    std::fs::create_dir_all(prompt_path.parent().unwrap()).unwrap();
    std::fs::write(&prompt_path, "Return only JSON.").unwrap();

    test.codex
        .submit(Op::RawrAutoCompactionJudgment {
            request_id: "req-1".to_string(),
            tier: "asap".to_string(),
            percent_remaining: 12,
            boundaries_present: vec!["plan_update".to_string()],
            last_agent_message: "done".to_string(),
            decision_prompt_path: "rawr/prompts/judgment.md".to_string(),
        })
        .await
        .unwrap();

    loop {
        let ev = timeout(Duration::from_secs(30), test.codex.next_event())
            .await
            .expect("timeout waiting for event")
            .expect("event stream ended unexpectedly")
            .msg;

        match ev {
            EventMsg::RawrAutoCompactionJudgmentResult(result) => {
                assert_eq!(result.request_id, "req-1");
                assert_eq!(result.tier, "asap");
                assert_eq!(result.should_compact, false);
                assert_eq!(result.reason, "keep context together");
                break;
            }
            EventMsg::AgentMessage(_)
            | EventMsg::AgentMessageDelta(_)
            | EventMsg::AgentMessageContentDelta(_)
            | EventMsg::UserMessage(_)
            | EventMsg::TurnStarted(_)
            | EventMsg::TurnComplete(_)
            | EventMsg::TokenCount(_) => {
                panic!("judgment op should not emit transcript or turn lifecycle events: {ev:?}");
            }
            _ => continue,
        }
    }

    assert_eq!(mock.requests().len(), 1);
}

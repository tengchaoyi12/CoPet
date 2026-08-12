use copet_lib::task_opener::{codex_open_plan, execute_open_plan};

#[test]
fn valid_codex_session_prefers_thread_deep_link() {
    let plan = codex_open_plan(Some("019ff53e-539b-7053-af7c-01b608aa7059"));

    assert_eq!(
        plan.primary,
        vec![
            "open".to_string(),
            "codex://threads/019ff53e-539b-7053-af7c-01b608aa7059".to_string()
        ]
    );
    assert_eq!(
        plan.fallback,
        vec![
            "open".to_string(),
            "-b".to_string(),
            "com.openai.codex".to_string()
        ]
    );
}

#[test]
fn invalid_session_is_never_inserted_into_a_url() {
    let plan = codex_open_plan(Some("thread-1;open https://example.com"));

    assert!(plan.primary.is_empty());
    assert!(plan
        .fallback
        .iter()
        .all(|argument| !argument.contains("example.com")));
}

#[test]
fn failed_deep_link_falls_back_to_opening_the_codex_app() {
    let plan = codex_open_plan(Some("thread-1"));
    let mut calls = Vec::new();

    execute_open_plan(&plan, |command| {
        calls.push(command.to_vec());
        Ok(calls.len() == 2)
    })
    .unwrap();

    assert_eq!(calls, vec![plan.primary, plan.fallback]);
}

#[test]
fn failure_of_both_open_attempts_is_reported() {
    let plan = codex_open_plan(Some("thread-1"));

    let result = execute_open_plan(&plan, |_| Ok(false));

    assert!(result.is_err());
}

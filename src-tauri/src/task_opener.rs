use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexOpenPlan {
    pub primary: Vec<String>,
    pub fallback: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum TaskOpenError {
    #[error("当前平台不支持打开 Codex 任务")]
    UnsupportedPlatform,
    #[error("无法启动打开命令：{0}")]
    Command(#[from] io::Error),
    #[error("无法打开 Codex 任务或应用")]
    OpenFailed,
}

pub fn codex_thread_url(session_id: &str) -> Option<String> {
    (!session_id.is_empty()
        && session_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-'))
    .then(|| format!("codex://threads/{session_id}"))
}

pub fn codex_open_plan(session_id: Option<&str>) -> CodexOpenPlan {
    let primary = session_id
        .and_then(codex_thread_url)
        .map(|url| vec!["open".to_string(), url])
        .unwrap_or_default();
    CodexOpenPlan {
        primary,
        fallback: vec![
            "open".to_string(),
            "-b".to_string(),
            "com.openai.codex".to_string(),
        ],
    }
}

pub fn execute_open_plan(
    plan: &CodexOpenPlan,
    mut run: impl FnMut(&[String]) -> io::Result<bool>,
) -> Result<(), TaskOpenError> {
    if !plan.primary.is_empty() && run(&plan.primary)? {
        return Ok(());
    }
    if run(&plan.fallback)? {
        return Ok(());
    }
    Err(TaskOpenError::OpenFailed)
}

#[cfg(target_os = "macos")]
pub fn open_codex_task(session_id: Option<&str>) -> Result<(), TaskOpenError> {
    use std::process::Command;

    let plan = codex_open_plan(session_id);
    execute_open_plan(&plan, |command| {
        let Some((program, arguments)) = command.split_first() else {
            return Ok(false);
        };
        Command::new(program)
            .args(arguments)
            .status()
            .map(|status| status.success())
    })
}

#[cfg(not(target_os = "macos"))]
pub fn open_codex_task(_session_id: Option<&str>) -> Result<(), TaskOpenError> {
    Err(TaskOpenError::UnsupportedPlatform)
}

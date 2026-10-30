use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;

use super::error::{Error, Result};
use super::model::{JobPhase, ToolId};

/// 正在进行或刚结束的任务，界面靠它显示进度条和取消按钮。
pub struct Job {
    pub id: String,
    pub tool: ToolId,
    pub version: String,
    pub kind: JobKind,
    pub cancel: Arc<AtomicBool>,
    pub started: Instant,
    pub finished: Mutex<Option<Finish>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    Install,
    Uninstall,
    Takeover,
    Import,
}

#[derive(Debug, Clone)]
pub struct Finish {
    pub phase: JobPhase,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub id: String,
    pub tool: ToolId,
    pub version: String,
    pub kind: JobKind,
    pub phase: JobPhase,
    pub running: bool,
    pub elapsed_secs: u64,
    pub message: Option<String>,
}

static JOBS: Mutex<Vec<Arc<Job>>> = Mutex::new(Vec::new());
static NEXT: Mutex<u64> = Mutex::new(1);

pub fn register(tool: ToolId, version: &str, kind: JobKind) -> Arc<Job> {
    let id = {
        let mut n = NEXT.lock().unwrap_or_else(|e| e.into_inner());
        let id = format!("job-{n}");
        *n += 1;
        id
    };
    let job = Arc::new(Job {
        id,
        tool,
        version: version.to_string(),
        kind,
        cancel: Arc::new(AtomicBool::new(false)),
        started: Instant::now(),
        finished: Mutex::new(None),
    });
    let mut jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
    // 只保留最近 30 条历史，正在跑的除外。
    jobs.retain(|j| !j.is_done() || j.started.elapsed().as_secs() < 3600);
    while jobs.len() >= 30 {
        jobs.remove(0);
    }
    jobs.push(Arc::clone(&job));
    job
}

impl Job {
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn done_phase(&self) -> Option<JobPhase> {
        self.finished.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|f| f.phase)
    }

    pub fn is_done(&self) -> bool {
        self.finished.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    pub fn finish(&self, phase: JobPhase, message: Option<String>) {
        *self.finished.lock().unwrap_or_else(|e| e.into_inner()) = Some(Finish { phase, message });
    }

    pub fn view(&self) -> JobView {
        let done = self.done_phase();
        JobView {
            id: self.id.clone(),
            tool: self.tool,
            version: self.version.clone(),
            kind: self.kind,
            phase: done.unwrap_or(JobPhase::Downloading),
            running: done.is_none(),
            elapsed_secs: self.started.elapsed().as_secs(),
            message: self
                .finished
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .and_then(|f| f.message.clone()),
        }
    }
}

pub fn cancel(job_id: &str) -> Result<()> {
    let jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
    let job = jobs
        .iter()
        .find(|j| j.id == job_id)
        .ok_or_else(|| Error::NotFound(format!("任务 {job_id} 不在了")))?;
    job.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

pub fn list() -> Vec<JobView> {
    JOBS.lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|j| j.view())
        .collect()
}

pub fn running_count() -> usize {
    JOBS.lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|j| !j.is_done())
        .count()
}

/// 供安装流程使用：把取消标记转成错误。
pub fn checkpoint(job: &Job) -> Result<()> {
    if job.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

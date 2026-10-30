use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use super::error::{Error, Result};
use super::model::{JobEvent, JobPhase, ToolId};
use super::util;

/// 下载一个包：Range 分片并发 + 断点续传 + 取消 + 进度回调 + sha256 校验。
#[derive(Debug, Clone)]
pub struct Plan {
    pub url: String,
    pub file_name: String,
    /// 最终落位（cache/<file_name>）。
    pub dest: PathBuf,
    /// 续传用的临时位置（cache/partial/<file_name>）。
    pub partial: PathBuf,
    pub expected_sha256: Option<String>,
    pub conns: usize,
}

pub type OnEvent = Arc<dyn Fn(&JobEvent) + Send + Sync + 'static>;

pub struct Reporter {
    job_id: String,
    tool: Option<ToolId>,
    version: Option<String>,
    on_event: Option<OnEvent>,
    last_emit: std::sync::Mutex<Instant>,
    started: Instant,
}

impl Reporter {
    fn new(
        job_id: &str,
        tool: Option<ToolId>,
        version: Option<String>,
        on_event: Option<OnEvent>,
    ) -> Arc<Self> {
        Arc::new(Self {
            job_id: job_id.to_string(),
            tool,
            version,
            on_event,
            last_emit: std::sync::Mutex::new(Instant::now() - Duration::from_secs(1)),
            started: Instant::now(),
        })
    }

    fn stage(&self, phase: JobPhase, message: Option<String>) {
        self.send(phase, 0, None, 0, None, None, message, false);
    }

    fn progress(&self, received: u64, total: Option<u64>) {
        let elapsed = self.started.elapsed().as_secs_f64().max(0.001);
        let rate = (received as f64 / elapsed) as u64;
        let eta = total
            .filter(|t| *t >= received && rate > 0)
            .map(|t| (t - received) / rate);
        let percent = total.map(|t| if t > 0 { received as f64 * 100.0 / t as f64 } else { 0.0 });
        self.send(
            JobPhase::Downloading,
            received,
            total,
            rate,
            eta,
            percent,
            if rate > 0 { Some(format!("{}/秒", util::human_bytes(rate))) } else { None },
            true,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn send(
        &self,
        phase: JobPhase,
        received: u64,
        total: Option<u64>,
        rate: u64,
        eta: Option<u64>,
        percent: Option<f64>,
        message: Option<String>,
        throttled: bool,
    ) {
        if throttled {
            let mut last = self.last_emit.lock().unwrap_or_else(|e| e.into_inner());
            if last.elapsed() < Duration::from_millis(200) {
                return;
            }
            *last = Instant::now();
        }
        if let Some(cb) = &self.on_event {
            cb(&JobEvent {
                job_id: self.job_id.clone(),
                tool: self.tool,
                version: self.version.clone(),
                phase,
                received,
                total,
                bytes_per_sec: rate,
                eta_secs: eta,
                percent,
                message,
            });
        }
    }
}

/// 跑完整个下载，返回最终文件路径。已有同 sha256 的包直接复用，不重复下载。
pub async fn run(
    client: &reqwest::Client,
    plan: Plan,
    job_id: &str,
    tool: Option<ToolId>,
    version: Option<String>,
    on_event: Option<OnEvent>,
    cancel: Arc<AtomicBool>,
) -> Result<PathBuf> {
    let rep = Reporter::new(job_id, tool, version, on_event);

    if let Some(dir) = plan.dest.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    tokio::fs::create_dir_all(
        plan.partial
            .parent()
            .ok_or_else(|| Error::Detail("下载临时路径不合法".to_string()))?,
    )
    .await?;

    if plan.dest.is_file() && verify_file(&plan.dest, plan.expected_sha256.as_deref()).await.is_ok()
    {
        rep.stage(
            JobPhase::Verifying,
            Some("这个包之前下过了，直接用".to_string()),
        );
        return Ok(plan.dest.clone());
    }

    let meta = probe(client, &plan.url).await?;
    let use_ranges = meta.accept_ranges && meta.total.unwrap_or(0) > 4 * 1024 * 1024;
    let conns = if use_ranges { plan.conns.clamp(1, 8) } else { 1 };

    let result = match conns {
        0 | 1 => single(client, &plan, &meta, &rep, &cancel).await,
        n => segmented(client, &plan, &meta, n, &rep, &cancel).await,
    };
    if let Err(e) = result {
        return Err(e);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }

    rep.stage(JobPhase::Verifying, None);
    verify_file(&plan.dest, plan.expected_sha256.as_deref())
        .await
        .map_err(|e| e.with_detail(format!("校验 {}", plan.file_name)))?;
    Ok(plan.dest.clone())
}

#[derive(Debug, Clone)]
struct RemoteMeta {
    total: Option<u64>,
    accept_ranges: bool,
}

async fn probe(client: &reqwest::Client, url: &str) -> Result<RemoteMeta> {
    let resp = client
        .head(url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("问不到远端文件信息：{e}")))?;
    let status = resp.status();
    if !status.is_success() && status.as_u16() != 405 {
        return Err(Error::NotFound(format!("远端说没有这个文件（{}）", status.as_u16())));
    }
    let headers = resp.headers();
    let total = headers
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|n| *n > 0);
    let accept_ranges = headers
        .get(reqwest::header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("bytes"))
        .unwrap_or(false);
    Ok(RemoteMeta { total, accept_ranges })
}

async fn single(
    client: &reqwest::Client,
    plan: &Plan,
    meta: &RemoteMeta,
    rep: &Arc<Reporter>,
    cancel: &Arc<AtomicBool>,
) -> Result<()> {
    let mut resume_from = tokio::fs::metadata(&plan.partial).await.map(|m| m.len()).unwrap_or(0);
    if resume_from > 0 && !meta.accept_ranges {
        // 服务端不认续传，只能重头下。
        let _ = tokio::fs::remove_file(&plan.partial).await;
        resume_from = 0;
    }

    let mut req = client.get(&plan.url);
    if resume_from > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
    }
    let resp = req.send().await.map_err(|e| Error::Network(format!("下载失败：{e}")))?;
    if resp.status().as_u16() == 416 && resume_from > 0 {
        // 临时文件已经和远端一样长，交给后面的校验判定好坏。
        return finish(plan).await;
    }
    if !resp.status().is_success() && resp.status().as_u16() != 206 {
        return Err(Error::NotFound(format!("下载被拒绝（{}）", resp.status().as_u16())));
    }

    // 要了续传却拿到 200，说明服务端没理 Range，必须重头写，否则拼出来是坏包。
    let mut file = if resp.status().as_u16() == 206 && resume_from > 0 {
        tokio::fs::OpenOptions::new().append(true).open(&plan.partial).await?
    } else {
        resume_from = 0;
        tokio::fs::File::create(&plan.partial).await?
    };

    let mut received = resume_from;
    let total = meta.total.or_else(|| resp.content_length().map(|c| c + resume_from));
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let chunk = chunk.map_err(|e| Error::Network(format!("下载断了：{e}")))?;
        file.write_all(&chunk).await?;
        received += chunk.len() as u64;
        rep.progress(received, total);
    }
    file.flush().await?;
    drop(file);
    finish(plan).await
}

async fn segmented(
    client: &reqwest::Client,
    plan: &Plan,
    meta: &RemoteMeta,
    conns: usize,
    rep: &Arc<Reporter>,
    cancel: &Arc<AtomicBool>,
) -> Result<()> {
    let total = meta.total.ok_or_else(|| {
        Error::Detail("远端不肯报文件大小，这次改成单线程下载".to_string())
    })?;

    // 预铺一个和远端等长的稀疏文件，每片只写自己那段，中断后按段继续。
    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&plan.partial)
        .await?;
    file.set_len(total).await?;
    drop(file);

    let per = total.div_ceil(conns as u64);
    let written = Arc::new(AtomicU64::new(0));
    let mut joinset = tokio::task::JoinSet::new();

    for i in 0..conns as u64 {
        let start = i * per;
        if start >= total {
            break;
        }
        let end = (start + per - 1).min(total - 1);
        joinset.spawn(segment_worker(
            client.clone(),
            plan.url.clone(),
            plan.partial.clone(),
            start,
            end,
            Arc::clone(&written),
            Arc::clone(cancel),
        ));
    }

    let monitor = tokio::spawn({
        let rep = Arc::clone(rep);
        let written = Arc::clone(&written);
        async move {
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                rep.progress(written.load(Ordering::Relaxed), Some(total));
            }
        }
    });

    let mut first_error: Option<Error> = None;
    while let Some(joined) = joinset.join_next().await {
        match joined {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
                // 有一片失败就别再耗下去了。
                cancel.store(true, Ordering::Relaxed);
            }
            Err(e) if e.is_cancelled() => {}
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(Error::Detail(format!("下载线程挂了：{e}")));
                }
            }
        }
    }
    monitor.abort();
    rep.progress(written.load(Ordering::Relaxed), Some(total));

    if let Some(e) = first_error {
        return Err(e);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    finish(plan).await
}

async fn segment_worker(
    client: reqwest::Client,
    url: String,
    path: PathBuf,
    start: u64,
    end: u64,
    written: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
) -> Result<()> {
    let resp = client
        .get(&url)
        .header(reqwest::header::RANGE, format!("bytes={start}-{end}"))
        .send()
        .await
        .map_err(|e| Error::Network(format!("分片连不上：{e}")))?;
    if resp.status().as_u16() != 206 {
        return Err(Error::Detail(format!(
            "远端不支持分段下载（返回 {}）",
            resp.status().as_u16()
        )));
    }
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .read(true)
        .open(&path)
        .await?;
    file.seek(std::io::SeekFrom::Start(start)).await?;

    let mut stream = resp.bytes_stream();
    let mut expect = start;
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let chunk = chunk.map_err(|e| Error::Network(format!("分片断了：{e}")))?;
        file.write_all(&chunk).await?;
        expect += chunk.len() as u64;
        written.fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }
    file.flush().await?;
    if expect != end + 1 {
        return Err(Error::Detail(format!(
            "这一段没下全（差 {}）",
            util::human_bytes(end + 1 - expect)
        )));
    }
    Ok(())
}

async fn finish(plan: &Plan) -> Result<()> {
    if plan.partial.is_file() {
        let _ = tokio::fs::remove_file(&plan.dest).await;
        tokio::fs::rename(&plan.partial, &plan.dest)
            .await
            .map_err(|e| Error::Io(format!("整理下载文件失败：{e}")))?;
    }
    if !plan.dest.is_file() {
        return Err(Error::NotFound("下载显示完成了，但文件不在".to_string()));
    }
    Ok(())
}

/// 流式算 sha256，几 GB 的包也不会整个读进内存。
pub async fn sha256_of(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| Error::Io(format!("打不开 {}：{e}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn sha256_of_blocking(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)
        .map_err(|e| Error::Io(format!("打不开 {}：{e}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub async fn verify_file(path: &Path, expected: Option<&str>) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = sha256_of(path).await?;
    if actual.to_lowercase() != expected.trim().to_lowercase() {
        return Err(Error::Verify {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

/// 清掉 N 天前的旧压缩包，避免缓存越攒越满。
pub async fn prune_cache(cache: &Path, keep_days: i64) -> usize {
    let Ok(entries) = std::fs::read_dir(cache) else {
        return 0;
    };
    let cutoff = chrono::Local::now() - chrono::TimeDelta::days(keep_days.max(0));
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .map(chrono::DateTime::<chrono::Local>::from)
            .unwrap_or(chrono::Local::now());
        if modified < cutoff {
            let _ = std::fs::remove_file(&path);
            removed += 1;
        }
    }
    removed
}

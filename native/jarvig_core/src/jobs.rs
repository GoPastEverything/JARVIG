//! In-process job queue. Not a second engine and not a worker process.
//!
//! A job reads the inputs it was given and writes a private result. The host
//! publishes that result on the thread that owns the world and the device.
//! `JARVIGWorker.exe` is not this queue. A later process can run the same
//! description. It is not built here.

use std::any::Any;
use std::collections::{BTreeMap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crate::asset::AssetId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JobId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Running,
    CancelRequested,
    Cancelled,
    Completed,
    Failed,
}

impl JobState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Running => "Running",
            Self::CancelRequested => "CancelRequested",
            Self::Cancelled => "Cancelled",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
        }
    }

    pub fn finished(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug)]
pub struct JobDesc {
    pub name: String,
    pub asset: Option<AssetId>,
    /// Host key: asset, source fingerprint, builder version. The queue stores it and does not read a disk cache.
    pub cache_key: Option<String>,
    pub dependencies: Vec<JobId>,
}

#[derive(Clone, Debug)]
pub struct JobSnapshot {
    pub id: JobId,
    pub name: String,
    pub asset: Option<AssetId>,
    pub cache_key: Option<String>,
    pub state: JobState,
    pub progress: f32,
    pub stage: String,
    pub elapsed_ms: u64,
    pub dependencies: Vec<JobId>,
    pub error: Option<String>,
}

pub struct JobContext {
    cancel: Arc<std::sync::atomic::AtomicBool>,
    live: Arc<Mutex<LiveProgress>>,
}

impl JobContext {
    pub fn cancel_requested(&self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn report(&self, fraction: f32, stage: &str) {
        if let Ok(mut live) = self.live.lock() {
            live.fraction = fraction.clamp(0.0, 1.0);
            live.stage = stage.to_string();
        }
    }
}

struct LiveProgress {
    fraction: f32,
    stage: String,
}

struct JobRecord {
    desc: JobDesc,
    state: JobState,
    enqueued: bool,
    submitted: Instant,
    started: Option<Instant>,
    finished: Option<Instant>,
    error: Option<String>,
    result: Option<Box<dyn Any + Send>>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    live: Arc<Mutex<LiveProgress>>,
    task: Option<Box<dyn FnOnce(&JobContext) -> Result<Box<dyn Any + Send>, String> + Send>>,
}

struct Inner {
    next: u64,
    shutdown: bool,
    jobs: BTreeMap<u64, JobRecord>,
    queue: VecDeque<u64>,
}

pub struct JobManager {
    inner: Arc<Mutex<Inner>>,
    wake: Arc<Condvar>,
    threads: Vec<JoinHandle<()>>,
}

impl JobManager {
    pub fn new(workers: usize) -> Self {
        Self::named("jarvig-job", workers)
    }

    /// A second pool does not see the first pool's queue. The editor keeps the sum of workers at most four.
    pub fn named(prefix: &str, workers: usize) -> Self {
        let workers = workers.clamp(1, 4);
        let inner = Arc::new(Mutex::new(Inner { next: 1, shutdown: false, jobs: BTreeMap::new(), queue: VecDeque::new() }));
        let wake = Arc::new(Condvar::new());
        let mut threads = Vec::with_capacity(workers);
        for index in 0..workers {
            let inner = Arc::clone(&inner);
            let wake = Arc::clone(&wake);
            let name = format!("{prefix}-{index}");
            let handle = thread::Builder::new()
                .name(name)
                .stack_size(16 * 1024 * 1024)
                .spawn(move || worker_loop(inner, wake))
                .expect("job worker");
            threads.push(handle);
        }
        Self { inner, wake, threads }
    }

    pub fn worker_count(&self) -> usize {
        self.threads.len()
    }

    /// Workers that have taken a job and have not finished it.
    pub fn busy_count(&self) -> usize {
        let guard = self.inner.lock().expect("job queue");
        guard.jobs.values().filter(|job| matches!(job.state, JobState::Running | JobState::CancelRequested)).count()
    }

    pub fn submit<T, F>(&self, desc: JobDesc, work: F) -> JobId
    where
        T: Send + 'static,
        F: FnOnce(&JobContext) -> Result<T, String> + Send + 'static,
    {
        let task = Box::new(move |ctx: &JobContext| work(ctx).map(|value| Box::new(value) as Box<dyn Any + Send>));
        let mut guard = self.inner.lock().expect("job queue");
        let id = guard.next;
        guard.next = guard.next.saturating_add(1);
        let ready = dependencies_ready(&guard, &desc.dependencies);
        let blocked = dependencies_blocked(&guard, &desc.dependencies);
        let mut record = JobRecord {
            desc,
            state: if blocked { JobState::Cancelled } else { JobState::Queued },
            enqueued: false,
            submitted: Instant::now(),
            started: None,
            finished: if blocked { Some(Instant::now()) } else { None },
            error: if blocked { Some("a dependency did not complete".into()) } else { None },
            result: None,
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            live: Arc::new(Mutex::new(LiveProgress { fraction: 0.0, stage: "Queued".into() })),
            task: if blocked { None } else { Some(task) },
        };
        if ready && !blocked {
            record.enqueued = true;
            guard.queue.push_back(id);
        }
        guard.jobs.insert(id, record);
        drop(guard);
        self.wake.notify_one();
        JobId(id)
    }

    pub fn cancel(&self, id: JobId) {
        let mut guard = self.inner.lock().expect("job queue");
        let Some(job) = guard.jobs.get_mut(&id.0) else { return };
        match job.state {
            JobState::Queued => {
                job.state = JobState::Cancelled;
                job.finished = Some(Instant::now());
                job.error = Some("cancelled".into());
                job.task = None;
                guard.queue.retain(|queued| *queued != id.0);
                release_dependents(&mut guard);
            }
            JobState::Running => {
                job.state = JobState::CancelRequested;
                job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            _ => {}
        }
        drop(guard);
        self.wake.notify_all();
    }

    pub fn snapshots(&self) -> Vec<JobSnapshot> {
        let guard = self.inner.lock().expect("job queue");
        guard.jobs.iter().map(|(id, job)| snapshot(*id, job)).collect()
    }

    pub fn take_result<T: Send + 'static>(&self, id: JobId) -> Option<Result<T, String>> {
        let mut guard = self.inner.lock().expect("job queue");
        let job = guard.jobs.get_mut(&id.0)?;
        match job.state {
            JobState::Completed => {
                let value = job.result.take()?;
                match value.downcast::<T>() {
                    Ok(boxed) => Some(Ok(*boxed)),
                    Err(_) => Some(Err("result type mismatch".into())),
                }
            }
            JobState::Failed | JobState::Cancelled => Some(Err(job.error.clone().unwrap_or_else(|| job.state.label().into()))),
            _ => None,
        }
    }

    /// Host progress for work that continues after the worker returns, such as a GPU upload.
    pub fn note(&self, id: JobId, fraction: f32, stage: &str) {
        let live = {
            let guard = self.inner.lock().expect("job queue");
            let Some(job) = guard.jobs.get(&id.0) else { return };
            Arc::clone(&job.live)
        };
        let mut progress = live.lock().unwrap_or_else(|poison| poison.into_inner());
        progress.fraction = fraction.clamp(0.0, 1.0);
        progress.stage = stage.to_string();
    }
}

impl Drop for JobManager {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.shutdown = true;
            for job in guard.jobs.values_mut() {
                job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        self.wake.notify_all();
        for handle in self.threads.drain(..) {
            let _ = handle.join();
        }
    }
}

fn worker_loop(inner: Arc<Mutex<Inner>>, wake: Arc<Condvar>) {
    loop {
        let next = {
            let mut guard = inner.lock().expect("job queue");
            loop {
                if guard.shutdown {
                    return;
                }
                if let Some(id) = guard.queue.pop_front() {
                    let Some(job) = guard.jobs.get_mut(&id) else { continue };
                    if job.state != JobState::Queued {
                        continue;
                    }
                    let Some(task) = job.task.take() else { continue };
                    job.state = JobState::Running;
                    job.started = Some(Instant::now());
                    let ctx = JobContext { cancel: Arc::clone(&job.cancel), live: Arc::clone(&job.live) };
                    break Some((id, task, ctx));
                }
                guard = wake.wait(guard).expect("job queue");
            }
        };
        let Some((id, task, ctx)) = next else { return };
        let outcome = catch_unwind(AssertUnwindSafe(|| task(&ctx)));
        let cancelled = ctx.cancel_requested();
        let mut guard = inner.lock().expect("job queue");
        if let Some(job) = guard.jobs.get_mut(&id) {
            job.finished = Some(Instant::now());
            match outcome {
                Ok(Ok(_value)) if cancelled => {
                    job.state = JobState::Cancelled;
                    job.error = Some("cancelled".into());
                }
                Ok(Ok(value)) => {
                    job.state = JobState::Completed;
                    job.result = Some(value);
                    if let Ok(mut live) = job.live.lock() {
                        live.fraction = 1.0;
                    }
                }
                Ok(Err(error)) if cancelled || error == "cancelled" => {
                    job.state = JobState::Cancelled;
                    job.error = Some(error);
                }
                Ok(Err(error)) => {
                    job.state = JobState::Failed;
                    job.error = Some(error);
                }
                Err(_) => {
                    job.state = JobState::Failed;
                    job.error = Some("the job panicked".into());
                }
            }
        }
        release_dependents(&mut guard);
        drop(guard);
        wake.notify_all();
    }
}

fn dependencies_ready(inner: &Inner, dependencies: &[JobId]) -> bool {
    dependencies.iter().all(|id| inner.jobs.get(&id.0).is_some_and(|job| job.state == JobState::Completed))
}

fn dependencies_blocked(inner: &Inner, dependencies: &[JobId]) -> bool {
    dependencies.iter().any(|id| {
        inner.jobs.get(&id.0).is_some_and(|job| matches!(job.state, JobState::Failed | JobState::Cancelled))
    })
}

fn release_dependents(inner: &mut Inner) {
    let ids: Vec<u64> = inner.jobs.keys().copied().collect();
    let mut ready = Vec::new();
    for id in ids {
        let Some(job) = inner.jobs.get(&id) else { continue };
        if job.state != JobState::Queued || job.enqueued || job.task.is_none() {
            continue;
        }
        if job.desc.dependencies.is_empty() {
            continue;
        }
        let deps: Vec<JobId> = job.desc.dependencies.clone();
        if dependencies_blocked(inner, &deps) {
            if let Some(job) = inner.jobs.get_mut(&id) {
                job.state = JobState::Cancelled;
                job.finished = Some(Instant::now());
                job.error = Some("a dependency did not complete".into());
                job.task = None;
            }
            continue;
        }
        if deps.iter().all(|dep| inner.jobs.get(&dep.0).is_some_and(|job| job.state == JobState::Completed)) {
            ready.push(id);
        }
    }
    for id in ready {
        if let Some(job) = inner.jobs.get_mut(&id) {
            job.enqueued = true;
            inner.queue.push_back(id);
        }
    }
}

fn snapshot(id: u64, job: &JobRecord) -> JobSnapshot {
    let (progress, stage) = job
        .live
        .lock()
        .map(|live| (live.fraction, live.stage.clone()))
        .unwrap_or((0.0, String::new()));
    let end = job.finished.unwrap_or_else(Instant::now);
    let start = job.started.unwrap_or(job.submitted);
    JobSnapshot {
        id: JobId(id),
        name: job.desc.name.clone(),
        asset: job.desc.asset,
        cache_key: job.desc.cache_key.clone(),
        state: job.state,
        progress,
        stage,
        elapsed_ms: end.saturating_duration_since(start).as_millis() as u64,
        dependencies: job.desc.dependencies.clone(),
        error: job.error.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn wait_until(manager: &JobManager, id: JobId, pred: impl Fn(&JobSnapshot) -> bool) -> JobSnapshot {
        for _ in 0..200 {
            if let Some(snap) = manager.snapshots().into_iter().find(|snap| snap.id == id) {
                if pred(&snap) {
                    return snap;
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("job {id:?} did not reach the expected state");
    }

    #[test]
    fn a_job_runs_off_the_caller_and_publishes_one_result() {
        let manager = JobManager::new(1);
        let id = manager.submit(JobDesc { name: "add".into(), asset: None, cache_key: None, dependencies: Vec::new() }, |_ctx| Ok::<u32, String>(7));
        let snap = wait_until(&manager, id, |snap| snap.state == JobState::Completed);
        assert_eq!(snap.name, "add");
        assert!((snap.progress - 1.0).abs() < 1.0e-4 || snap.progress == 0.0 || snap.progress == 1.0);
        assert_eq!(manager.take_result::<u32>(id).unwrap().unwrap(), 7);
        assert!(manager.take_result::<u32>(id).is_none());
    }

    #[test]
    fn a_queued_cancel_does_not_run_the_body() {
        let manager = JobManager::new(1);
        let gate = Arc::new(AtomicUsize::new(0));
        let started = Arc::clone(&gate);
        let blocker = manager.submit(JobDesc { name: "block".into(), asset: None, cache_key: None, dependencies: Vec::new() }, move |ctx| {
            started.store(1, Ordering::Relaxed);
            while !ctx.cancel_requested() {
                thread::sleep(Duration::from_millis(5));
            }
            Err::<u32, String>("cancelled".into())
        });
        wait_until(&manager, blocker, |snap| snap.state == JobState::Running);
        let ran = Arc::new(AtomicUsize::new(0));
        let flag = Arc::clone(&ran);
        let queued = manager.submit(
            JobDesc { name: "later".into(), asset: None, cache_key: Some("asset:fingerprint:v1".into()), dependencies: vec![blocker] },
            move |_ctx| {
                flag.store(1, Ordering::Relaxed);
                Ok::<u32, String>(1)
            },
        );
        manager.cancel(queued);
        let snap = wait_until(&manager, queued, |snap| snap.state.finished());
        assert_eq!(snap.state, JobState::Cancelled);
        assert_eq!(ran.load(Ordering::Relaxed), 0);
        assert_eq!(snap.cache_key.as_deref(), Some("asset:fingerprint:v1"));
        manager.cancel(blocker);
        wait_until(&manager, blocker, |snap| snap.state.finished());
    }

    #[test]
    fn two_managers_do_not_share_a_queue() {
        let general = JobManager::named("jarvig-job", 1);
        let einstein = JobManager::named("jarvig-einstein", 1);
        assert_eq!(general.worker_count(), 1);
        assert_eq!(einstein.worker_count(), 1);
        assert_eq!(general.busy_count(), 0);
        let id = einstein.submit(JobDesc { name: "Build Einstein Surface".into(), asset: None, cache_key: Some("micro:1".into()), dependencies: Vec::new() }, |_ctx| {
            Ok::<u32, String>(3)
        });
        let snap = wait_until(&einstein, id, |snap| snap.state == JobState::Completed);
        assert_eq!(snap.name, "Build Einstein Surface");
        assert!(general.snapshots().is_empty());
        assert_eq!(einstein.take_result::<u32>(id).unwrap().unwrap(), 3);
        assert_eq!(einstein.busy_count(), 0);
    }

    #[test]
    fn a_running_job_observes_cancel_and_a_failure_has_no_value() {
        let manager = JobManager::new(1);
        let id = manager.submit(JobDesc { name: "spin".into(), asset: None, cache_key: None, dependencies: Vec::new() }, |ctx| {
            ctx.report(0.25, "working");
            while !ctx.cancel_requested() {
                thread::sleep(Duration::from_millis(5));
            }
            Err::<u32, String>("cancelled".into())
        });
        wait_until(&manager, id, |snap| snap.state == JobState::Running && snap.progress >= 0.25);
        manager.cancel(id);
        let snap = wait_until(&manager, id, |snap| snap.state == JobState::Cancelled);
        assert_eq!(snap.stage, "working");
        assert!(manager.take_result::<u32>(id).unwrap().is_err());
        let failed = manager.submit(JobDesc { name: "bad".into(), asset: None, cache_key: None, dependencies: Vec::new() }, |_ctx| Err::<u32, String>("nope".into()));
        let snap = wait_until(&manager, failed, |snap| snap.state == JobState::Failed);
        assert_eq!(snap.error.as_deref(), Some("nope"));
        assert!(manager.take_result::<u32>(failed).unwrap().is_err());
    }
}

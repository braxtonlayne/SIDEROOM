//! Tokio job runner: per-kind concurrency, polling, graceful shutdown.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinSet;

use crate::BoxFuture;
use crate::jobs::{self, Job};

pub type HandlerError = Box<dyn std::error::Error + Send + Sync>;
type Handler = Arc<dyn Fn(Job) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>;

struct Registration {
    concurrency: usize,
    handler: Handler,
}

pub struct JobRunner {
    pool: PgPool,
    worker_id: String,
    poll_interval: Duration,
    kinds: HashMap<String, Registration>,
}

impl JobRunner {
    pub fn new(pool: PgPool, worker_id: impl Into<String>) -> Self {
        Self {
            pool,
            worker_id: worker_id.into(),
            poll_interval: Duration::from_secs(1),
            kinds: HashMap::new(),
        }
    }

    pub fn poll_interval(mut self, d: Duration) -> Self {
        self.poll_interval = d;
        self
    }

    /// Register a handler for `kind`, running at most `concurrency` jobs of it at once.
    pub fn register<F>(mut self, kind: impl Into<String>, concurrency: usize, handler: F) -> Self
    where
        F: Fn(Job) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync + 'static,
    {
        self.kinds.insert(
            kind.into(),
            Registration {
                concurrency: concurrency.max(1),
                handler: Arc::new(handler),
            },
        );
        self
    }

    /// Run until `shutdown` flips to `true`, then stop claiming and wait for
    /// in-flight jobs to finish.
    pub async fn run(self, shutdown: watch::Receiver<bool>) {
        let mut loops = JoinSet::new();
        for (kind, reg) in self.kinds {
            loops.spawn(kind_loop(
                self.pool.clone(),
                self.worker_id.clone(),
                kind,
                reg,
                self.poll_interval,
                shutdown.clone(),
            ));
        }
        while loops.join_next().await.is_some() {}
    }
}

async fn kind_loop(
    pool: PgPool,
    worker: String,
    kind: String,
    reg: Registration,
    poll: Duration,
    mut shutdown: watch::Receiver<bool>,
) {
    let sem = Arc::new(Semaphore::new(reg.concurrency));
    let kinds = vec![kind.clone()];
    loop {
        if *shutdown.borrow() {
            break;
        }
        let permit = tokio::select! {
            p = sem.clone().acquire_owned() => match p { Ok(p) => p, Err(_) => break },
            _ = shutdown.changed() => continue,
        };
        let job = match jobs::claim(&pool, &kinds, &worker, 1).await {
            Ok(mut v) => v.pop(),
            Err(e) => {
                tracing::error!(%kind, error = %e, "job claim failed");
                None
            }
        };
        let Some(job) = job else {
            drop(permit);
            tokio::select! {
                _ = tokio::time::sleep(poll) => {}
                _ = shutdown.changed() => {}
            }
            continue;
        };
        let pool = pool.clone();
        let handler = reg.handler.clone();
        tokio::spawn(async move {
            let id = job.id;
            // Inner spawn isolates handler panics.
            let outcome = match tokio::spawn(handler(job)).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(e.to_string()),
                Err(join) => Err(format!("handler panicked: {join}")),
            };
            let res = match outcome {
                Ok(()) => jobs::complete(&pool, id).await.map(|_| ()),
                Err(msg) => {
                    tracing::warn!(job_id = id, error = %msg, "job failed");
                    jobs::fail(&pool, id, &msg).await.map(|_| ())
                }
            };
            if let Err(e) = res {
                tracing::error!(job_id = id, error = %e, "recording job outcome failed");
            }
            drop(permit);
        });
    }
    // Drain: wait until every in-flight job has released its permit.
    let _ = sem.acquire_many(reg.concurrency as u32).await;
}

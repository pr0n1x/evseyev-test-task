use std::future::Future;
use futures::future::join_all;

pub struct Worker<F: Future> { cnt: usize, fut: Vec<Vec<F>> }

impl<F: Future + 'static> Worker<F> {
    pub fn new() -> Self {
        Self::with_batches(std::thread::available_parallelism().unwrap().get())
    }

    pub fn with_batches(batches: usize) -> Self {
        let mut worker = Self {
            cnt: batches,
            fut: Vec::with_capacity(batches)
        };
        for _ in 0..worker.cnt {
            worker.fut.push(Vec::new());
        }
        worker
    }

    pub fn push(&mut self, fut: F) {
        let task_index = self.fut.len() % self.cnt;
        self.fut[task_index].push(fut)
    }

    pub async fn run(self) where F: Send {
        let mut handles = Vec::new();
        for jobs in self.fut {
            handles.push(tokio::spawn(async move {
                for job in jobs { job.await; }
            }));
        }
        join_all(handles).await;
    }

    pub async fn run_single_threaded(self, batch_size: Option<usize>) {
        let batch_size = match batch_size { Some(x) => x, None => self.cnt };
        let flat = self.fut.into_iter().flatten().collect::<Vec<_>>();
        let fut_count = flat.len();
        let chunks = if fut_count <= batch_size {
            vec![flat]
        } else {
            let chunks_tail = match fut_count % batch_size > 0 {
                true => 1, false => 0
            };
            let chunks_count = fut_count / batch_size + chunks_tail;
            let mut chunks = Vec::with_capacity(chunks_count);
            for _ in 0..chunks_count {
                chunks.push(Vec::new());
            }
            for (i, fut) in flat.into_iter().enumerate() {
                chunks[i % chunks_count].push(fut)
            }
            chunks
        };
        for jobs in chunks {
            join_all(jobs).await;
        }
    }
}

impl<F: Future + Send + 'static> Worker<F>
    where F::Output: Send + Sync
{
    pub async fn run_and_collect_results(self) -> Vec<F::Output> {
        let mut handles = Vec::new();
        for jobs in self.fut {
            handles.push(tokio::spawn(async move {
                let mut res = Vec::new();
                for job in jobs { res.push(job.await); }
                res
            }));
        }
        let mut res = Vec::new();
        for join_result in join_all(handles).await {
            for job_result in join_result.unwrap().into_iter() {
                res.push(job_result)
            }
        }
        res
    }

    pub async fn run_all_joined(self) {
        let mut handles = Vec::new();
        for jobs in self.fut {
            handles.push(tokio::spawn(async move {
                join_all(jobs).await
            }));
        }
        join_all(handles).await;
    }

    pub async fn run_all_joined_and_collect_results(self) -> Vec<F::Output> {
        let mut handles = Vec::new();
        for jobs in self.fut {
            handles.push(tokio::spawn(async move {
                join_all(jobs).await
            }));
        }
        let mut res = Vec::new();
        for join_result in join_all(handles).await {
            for job_result in join_result.unwrap().into_iter() {
                res.push(job_result)
            }
        }
        res
    }
}

/// The operator's own namespace, injected by the chart via the downward API
/// (`fieldRef: metadata.namespace`).
pub const OPERATOR_NAMESPACE_ENV: &str = "AUTHROUTE_NAMESPACE";

/// Address the controller's HTTP server (`/metrics`, `/healthz`, `/readyz`)
/// binds to. Matches the chart's `controller.probePort` (8081).
pub const HTTP_ADDR: &str = "0.0.0.0:8081";

/// Number of tokio worker threads the controller runtime runs. The std default (`available_parallelism`) sizes
/// the pool to the HOST core count, NOT the cgroup CPU quota, so on a large node it
/// spawns dozens of worker threads, each carrying a ~2 MiB stack AND a glibc malloc
/// arena that retains freed memory — inflating RSS for no throughput gain. The chart
/// sets this from `controller.workerThreads`; defaults to [`DEFAULT_WORKER_THREADS`].
pub const WORKER_THREADS_ENV: &str = "AUTHROUTE_WORKER_THREADS";

/// Fallback worker-thread count when [`WORKER_THREADS_ENV`] is unset/unparseable.
/// Two covers the controller's concurrency comfortably; raise it via the chart for
/// a reconcile-heavy deployment.
pub const DEFAULT_WORKER_THREADS: usize = 2;

/// Resolve the tokio worker-thread count from [`WORKER_THREADS_ENV`], clamped to at
/// least 1 (tokio's runtime builder panics on 0), falling back to
/// [`DEFAULT_WORKER_THREADS`] when unset or unparseable.
pub fn worker_threads() -> usize {
    std::env::var(WORKER_THREADS_ENV)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|n| n.max(1))
        .unwrap_or(DEFAULT_WORKER_THREADS)
}

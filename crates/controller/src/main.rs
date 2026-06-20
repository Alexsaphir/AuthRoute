#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> std::process::ExitCode {
    // Explicit worker-thread count instead of `Runtime::new()`'s default
    // (`available_parallelism` = host cores, ignoring the cgroup CPU quota): the
    // controller is I/O-bound, so a small pool is ample and avoids spawning a worker
    // thread — each with a stack and its own malloc arena — per host core on big nodes.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(authroute_controller::config::worker_threads())
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to start tokio runtime: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match runtime.block_on(authroute_controller::run()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("controller exited with error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

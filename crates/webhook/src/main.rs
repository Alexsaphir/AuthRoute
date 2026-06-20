fn main() -> std::process::ExitCode {
    // Small fixed pool: the webhook is I/O-bound (admission callouts + a CEL/regex
    // compile per request), so a host-core-sized default would only inflate RSS.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to start tokio runtime: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match runtime.block_on(authroute_webhook::run()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("webhook exited with error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
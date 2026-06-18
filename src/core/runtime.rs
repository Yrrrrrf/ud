pub const MAX_CONCURRENT_REQUESTS: usize = 12;

pub fn init_tracing(verbose: bool) {
    let filter = if verbose { "ud=debug,info" } else { "ud=off" };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init()
        .ok();
}

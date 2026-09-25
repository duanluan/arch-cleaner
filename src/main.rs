fn main() {
    reset_sigpipe_handling();

    match arch_cleaner::cli::run_from_env() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

/// Rust ignores `SIGPIPE` by default, so piping our stdout into tools such as
/// `head` or `jq` that exit early would make `println!` panic with
/// "failed printing to stdout: Broken pipe". Restore the default disposition
/// so the process dies quietly like any other CLI tool.
#[cfg(unix)]
fn reset_sigpipe_handling() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe_handling() {}

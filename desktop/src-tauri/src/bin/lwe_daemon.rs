//! Headless control daemon — Rust replacement for tooling/lwe-daemon.py
//! Listens on http://127.0.0.1:45127

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Err(e) = rt.block_on(wpengine_lib::http_daemon::run_forever()) {
        eprintln!("lwe-daemon fatal: {e}");
        std::process::exit(1);
    }
}

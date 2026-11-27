//! Start the browser E2E bridge.
//!
//! The Playwright suite starts this binary. The bridge prints `BRIDGE_URL` on
//! one line, then serves the built UI and the JSON command surface.

#[tokio::main]
async fn main() {
    if let Err(error) = swap_e2e::bridge::run().await {
        eprintln!("the E2E bridge failed: {error}");
        std::process::exit(1);
    }
}

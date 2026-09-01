fn main() {
    // rust-embed fails to compile if the target dir is missing.
    std::fs::create_dir_all("../../web/dist").ok();
    println!("cargo::rerun-if-changed=../../web/dist");

    // Without this a release build embeds nothing and 404s at runtime, and only
    // `cargo test` would catch it — not `cargo install` or `cargo build -r`.
    if !std::path::Path::new("../../web/dist/index.html").exists() {
        let level = match std::env::var("PROFILE").as_deref() {
            Ok("release") => "error",
            _ => "warning",
        };
        println!("cargo::{level}=web/dist is empty — run `npm ci && npm run build` in web/");
    }
}

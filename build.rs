fn main() {
    println!(
        "cargo::rustc-env=REPORT_HOST={}",
        std::env::var("HOST").expect("Cargo sets HOST")
    );
    println!("cargo::rerun-if-changed=build.rs");
}

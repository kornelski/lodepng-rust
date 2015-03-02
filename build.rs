#![feature(process)]
#![feature(env)]
fn main() {
    if !std::process::Command::new("make")
        .status().unwrap().success() {
        panic!("Script failed");
    }
    println!("cargo:rustc-flags=-L {}", std::env::var("OUT_DIR").unwrap());
}

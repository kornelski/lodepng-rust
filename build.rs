#![feature(io)]
fn main() {
    if !std::old_io::Command::new("make")
        .stdout(::std::old_io::process::InheritFd(1))
        .stderr(::std::old_io::process::InheritFd(2))
        .status().unwrap().success() {
        panic!("Script failed");
    }
    println!("cargo:rustc-flags=-L {}", std::env::var_string("OUT_DIR").unwrap());
}

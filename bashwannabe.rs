use std::io::Command;

#[cfg(win32)]
fn main() {
    Command::new("mingw32-make").status().unwrap();
}

#[cfg(unix)]
fn main() {
    Command::new("make").status().unwrap();
}

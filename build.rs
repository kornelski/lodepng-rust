extern crate cc;

fn main() {
    cc::Build::new().file("vendor/lodepng.c").compile("lodepng");
}

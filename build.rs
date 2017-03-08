extern crate gcc;

fn main() {
    gcc::compile_library("liblodepng.a", &["vendor/lodepng.c"]);
}

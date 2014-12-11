
extern crate gcc;

use std::default::Default;

fn main() {
	gcc::compile_library("liblodepng.a", &Default::default(), &["src/lodepng.c"]);
}

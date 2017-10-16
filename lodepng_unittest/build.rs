extern crate cc;

fn main() {
    cc::Build::new()
        .cpp(true)
        .file("vendor/lodepng_unittest.cpp")
        .file("vendor/lodepng_util.cpp")
        .file("vendor/lodepng.cpp")
        .opt_level(3)
        .compile("lodeunittest");
}

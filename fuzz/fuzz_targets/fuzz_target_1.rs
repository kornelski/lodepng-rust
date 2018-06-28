#![no_main]
#[macro_use] extern crate libfuzzer_sys;
extern crate lodepng;

fuzz_target!(|data: &[u8]| {
    lodepng::decode32(data);
});

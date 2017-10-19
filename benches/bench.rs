#![feature(test)]
extern crate test;
extern crate lodepng;

#[bench]
fn encode(bencher: &mut test::Bencher) {
    let img = lodepng::decode24_file("/tmp/test.png").unwrap();
    bencher.iter(|| {
        lodepng::encode_memory(&img.buffer, img.width, img.height, lodepng::ColorType::RGB, 8).unwrap()
    });
}

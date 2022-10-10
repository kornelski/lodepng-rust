#![feature(test)]
extern crate test;

#[bench]
fn roundtrip(bencher: &mut test::Bencher) {
    let mut data = vec![0u8; 640*480*3];
    for (i, px) in data.iter_mut().enumerate() {
        *px = ((i ^ (13 + i * 17) ^ (i * 13) ^ (i/113 * 11)) >> 5) as u8;
    }
    bencher.iter(|| {
        let res = lodepng::encode_memory(&data, 640, 480, lodepng::ColorType::RGB, 8).unwrap();
        lodepng::decode32(res)
    });
}

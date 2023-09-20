#![feature(test)]
use lodepng::Encoder;

extern crate test;

#[bench]
fn roundtrip(bencher: &mut test::Bencher) {
    let mut data = vec![0u8; 640*480*3];
    for (i, px) in data.iter_mut().enumerate() {
        *px = ((i ^ (13 + i * 17) ^ (i * 13) ^ (i/113 * 11)) >> 5) as u8;
    }
    bencher.bytes = data.len() as _;
    bencher.iter(|| {
        let res = lodepng::encode_memory(&data, 640, 480, lodepng::ColorType::RGB, 8).unwrap();
        lodepng::decode32(res)
    });
}

#[bench]
fn decode_filter_0(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(0);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

#[bench]
fn decode_filter_1(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(1);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

#[bench]
fn decode_filter_3(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(3);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

#[bench]
fn decode_filter_4(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(4);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

fn test_png_with_filter(filter: u8) -> Vec<u8> {
    let mut data = vec![0u8; 640*480*3];
    for (i, px) in data.iter_mut().enumerate() {
        *px = ((i ^ (13 + i * 81) ^ (i * 3) ^ (i/113 * 11)) >> 7) as u8;
    }
    let mut state = Encoder::new();
    state.set_auto_convert(false);
    state.set_predefined_filters(vec![filter; 480]);
    state.info_raw_mut().colortype = lodepng::ColorType::RGB;
    state.info_raw_mut().set_bitdepth(8);
    state.info_png_mut().color.colortype = lodepng::ColorType::RGB;
    state.info_png_mut().color.set_bitdepth(8);
    state.encode(&data, 640, 480).unwrap()
}

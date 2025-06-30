#![feature(test)]
use lodepng::{ColorType, Encoder, FilterStrategy};

extern crate test;

#[bench]
fn roundtrip(bencher: &mut test::Bencher) {
    let mut data = vec![0u8; 640*480*3];
    for (i, px) in data.iter_mut().enumerate() {
        *px = ((i ^ (13 + i * 17) ^ (i * 13) ^ (i/113 * 11)) >> 5) as u8;
    }
    bencher.bytes = data.len() as _;
    bencher.iter(|| {
        let res = lodepng::encode_memory(&data, 640, 480, ColorType::RGB, 8).unwrap();
        lodepng::decode32(res)
    });
}

#[bench]
fn level_0_filter_0(bencher: &mut test::Bencher) {
    let data = pixels_to_filter(ColorType::RGB, 8);
    bencher.bytes = data.len() as _;
    bencher.iter(move || {
        encode_with_filter(0, FilterStrategy::PREDEFINED, 0, ColorType::RGB, 8, &data)
    });
}

#[bench]
fn level_0_filter_1(bencher: &mut test::Bencher) {
    let data = pixels_to_filter(ColorType::RGB, 8);
    bencher.bytes = data.len() as _;
    bencher.iter(move || {
        encode_with_filter(1, FilterStrategy::PREDEFINED, 0, ColorType::RGB, 8, &data)
    });
}

#[bench]
fn level_0_filter_2(bencher: &mut test::Bencher) {
    let data = pixels_to_filter(ColorType::RGB, 8);
    bencher.bytes = data.len() as _;
    bencher.iter(move || {
        encode_with_filter(2, FilterStrategy::PREDEFINED, 0, ColorType::RGB, 8, &data)
    });
}

#[bench]
fn level_0_filter_3(bencher: &mut test::Bencher) {
    let data = pixels_to_filter(ColorType::RGB, 8);
    bencher.bytes = data.len() as _;
    bencher.iter(move || {
        encode_with_filter(3, FilterStrategy::PREDEFINED, 0, ColorType::RGB, 8, &data)
    });
}

#[bench]
fn level_0_filter_4(bencher: &mut test::Bencher) {
    let data = pixels_to_filter(ColorType::RGB, 8);
    bencher.bytes = data.len() as _;
    bencher.iter(move || {
        encode_with_filter(4, FilterStrategy::PREDEFINED, 0, ColorType::RGB, 8, &data)
    });
}

#[bench]
fn level_6_filter_strategy_a_zero(bencher: &mut test::Bencher) {
    let data = pixels_to_filter(ColorType::RGB, 8);
    bencher.bytes = data.len() as _;
    bencher.iter(move || {
        encode_with_filter(0, FilterStrategy::ZERO, 6, ColorType::RGB, 8, &data)
    });
}

#[bench]
fn level_6_filter_strategy_b_minsum(bencher: &mut test::Bencher) {
    let data = pixels_to_filter(ColorType::RGB, 8);
    bencher.bytes = data.len() as _;
    bencher.iter(move || {
        encode_with_filter(0, FilterStrategy::MINSUM, 6, ColorType::RGB, 8, &data)
    });
}

#[bench]
fn level_6_filter_strategy_c_entropy(bencher: &mut test::Bencher) {
    let data = pixels_to_filter(ColorType::RGB, 8);
    bencher.bytes = data.len() as _;
    bencher.iter(move || {
        encode_with_filter(0, FilterStrategy::ENTROPY, 6, ColorType::RGB, 8, &data)
    });
}

#[bench]
fn level_6_filter_strategy_d_brute_force(bencher: &mut test::Bencher) {
    let data = pixels_to_filter(ColorType::RGB, 8);
    bencher.bytes = data.len() as _;
    bencher.iter(move || {
        encode_with_filter(0, FilterStrategy::BRUTE_FORCE, 6, ColorType::RGB, 8, &data)
    });
}

#[bench]
fn decode_level_8_filter_0(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(0, 8, ColorType::RGB, 8);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

#[bench]
fn decode_level_8_filter_1(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(1, 8, ColorType::RGB, 8);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

#[bench]
fn decode_level_8_filter_2(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(2, 8, ColorType::RGB, 8);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

#[bench]
fn decode_level_8_filter_3(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(3, 8, ColorType::RGB, 8);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

#[bench]
fn decode_level_8_filter_4_rgb(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(4, 8, ColorType::RGB, 8);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

#[bench]
fn decode_level_8_filter_4_rgb16(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(4, 8, ColorType::RGB, 16);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

#[bench]
fn decode_level_8_filter_4_rgba(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(4, 8, ColorType::RGBA, 8);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

#[bench]
fn decode_level_8_filter_4_g(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(4, 8, ColorType::GREY, 8);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

#[bench]
fn decode_level_8_filter_4_ga(bencher: &mut test::Bencher) {
    let res = test_png_with_filter(4, 8, ColorType::GREY_ALPHA, 8);
    bencher.bytes = res.len() as _;
    bencher.iter(|| {
        lodepng::decode24(&res)
    });
}

fn pixels_to_filter(color_type: ColorType, depth: u8) -> Vec<u8> {
    let mut data = vec![0u8; 640*480* color_type.bpp(depth.into()) as usize/8];
    for (i, px) in data.iter_mut().enumerate() {
        *px = ((i ^ (13 + i * 81) ^ (i * 3) ^ (i/113 * 11)) >> 7) as u8;
    }
    data
}

fn test_png_with_filter(filter: u8, level: u8, color_type: ColorType, depth: u8) -> Vec<u8> {
    encode_with_filter(filter, FilterStrategy::PREDEFINED, level, color_type, depth, &pixels_to_filter(color_type, depth))
}

#[inline(never)]
fn encode_with_filter(filter: u8, strategy: FilterStrategy, level: u8, color_type: ColorType, depth: u8, data: &[u8]) -> Vec<u8> {
    let mut state = Encoder::new();
    state.set_auto_convert(false);
    if strategy == FilterStrategy::PREDEFINED {
        state.set_predefined_filters(vec![filter; 480]);
    } else {
        state.set_filter_strategy(strategy, false);
    }
    state.info_raw_mut().colortype = color_type;
    state.info_raw_mut().set_bitdepth(depth.into());
    state.info_png_mut().color.colortype = color_type;
    state.info_png_mut().color.set_bitdepth(depth.into());
    state.settings_mut().set_level(level);
    state.encode(data, 640, 480).unwrap()
}

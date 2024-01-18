use lodepng::*;

// top-level files create new executables, which is slower
mod roundtrip {
    mod roundtrip_test;
}

fn encode<T: rgb::Pod>(pixels: &[T], in_type: ColorType, out_type: ColorType) -> Result<Vec<u8>, Error> {
    let mut state = Encoder::new();
    state.set_auto_convert(true);
    state.info_raw_mut().colortype = in_type;
    state.info_raw_mut().set_bitdepth(8);
    state.info_png_mut().color.colortype = out_type;
    state.info_png_mut().color.set_bitdepth(8);
    state.encode(pixels, pixels.len(), 1)
}

#[test]
fn bgr() {
    let png = encode(&[rgb::alt::BGR{r:1u8,g:2,b:3}], ColorType::BGR, ColorType::RGB).unwrap();
    let img = decode24(png).unwrap();
    assert_eq!(img.buffer[0], rgb::RGB{r:1,g:2,b:3});

    let png = encode(&[rgb::alt::BGRA{r:1u8,g:2,b:3,a:111u8}], ColorType::BGRX, ColorType::RGB).unwrap();
    let img = decode32(png).unwrap();
    assert_eq!(img.buffer[0], rgb::RGBA8{r:1,g:2,b:3,a:255});
}

#[test]
fn redecode1() {
    let img1 = decode_file("tests/graytest.png", ColorType::GREY, 8).unwrap();
    let img1 = match img1 {
        Image::Grey(a) => a,
        _ => panic!(),
    };
    let png = encode_memory(&img1.buffer, img1.width, img1.height, ColorType::GREY, 8).unwrap();
    let img2 = decode_memory(png, ColorType::GREY, 8).unwrap();
    let img2 = match img2 {
        Image::Grey(a) => a,
        _ => panic!(),
    };
    assert_eq!(img1.buffer, img2.buffer);
}

#[test]
fn redecode2() {
    let img1 = decode24_file("tests/fry-test.png").unwrap();
    let png = encode24(&img1.buffer, img1.width, img1.height).unwrap();
    let img2 = decode24(png).unwrap();

    assert_eq!(img1.buffer, img2.buffer);
}

#[test]
fn random() {
    let mut data = vec![0u8; 639*479*3];
    for (i, px) in data.iter_mut().enumerate() {
        *px = ((i ^ (13 + i * 17) ^ (i * 13) ^ (i/113 * 11)) >> 5) as u8;
    }

    let png = encode24(&data, 639, 479).unwrap();
    let img2 = decode24(png).unwrap();

    use rgb::*;
    assert_eq!(data.as_rgb(), &img2.buffer[..]);
}

#[test]
fn fourbit() {
    decode_file("tests/4bitgray.png", ColorType::GREY, 4).unwrap();
}

#[test]
fn bgra() {
    let png = encode(&[rgb::alt::BGRA{r:1u8,g:2,b:3,a:4u8}], ColorType::BGRA, ColorType::RGBA).unwrap();
    let img = decode32(png).unwrap();
    assert_eq!(img.buffer[0], rgb::RGBA8{r:1,g:2,b:3,a:4u8});
}

#[test]
#[ignore] // slow
fn huge() {
    let png = encode24(&vec![RGB::new(0u8,0,0); 67777*68888], 67777, 68888).unwrap();
    let img = decode24(png).unwrap();
    assert_eq!(img.buffer[0], RGB::new(0,0,0));
    assert_eq!(img.width, 67777);
    assert_eq!(img.height, 68888);
}

#[test]
fn rgb_with_trns_inspect() {
    let mut state = Encoder::new();
    state.info_raw_mut().colortype = ColorType::RGB;
    state.info_raw_mut().set_key(0,0,0);
    state.info_png_mut().color.colortype = ColorType::RGB;
    state.info_png_mut().color.set_key(0,0,0);
    state.set_auto_convert(false);
    let png_data = state.encode(&[1u8,2,3,0,0,0], 2, 1).unwrap();

    let mut decoder = lodepng::Decoder::new();
    decoder.decode(&png_data).unwrap();
    assert_eq!(decoder.info_png().color.colortype, ColorType::RGB);
    assert!(decoder.info_png().color.can_have_alpha());

    let mut decoder = lodepng::Decoder::new();
    decoder.inspect(&png_data).unwrap();
    assert_eq!(decoder.info_png().color.colortype, ColorType::RGB);
    assert!(decoder.info_png().color.can_have_alpha());
}

#[test]
fn text_chunks() {
    let mut s = Encoder::new();
    s.set_text_compression(false);
    let longstr = "World 123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_";
    assert!(longstr.len() > 89);
    s.info_png_mut().add_text("Hello", longstr).unwrap();
    assert_eq!(1, s.info_png().text_keys().count());
    let data = s.encode(&[0], 1, 1).unwrap();

    assert!(data.windows(4).any(|w| w == b"tEXt"));

    let mut s = Decoder::new();
    s.read_text_chunks(true);
    s.decode(data).unwrap();
    assert_eq!(1, s.info_png().text_keys().count());
}

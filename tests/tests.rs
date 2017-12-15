extern crate lodepng;
extern crate rgb;
use lodepng::*;

fn encode<T: Copy>(pixels: &[T], in_type: ColorType, out_type: ColorType) -> Result<Vec<u8>, Error> {
    let mut state = State::new();
    state.set_auto_convert(true);
    state.info_raw.colortype = in_type;
    state.info_raw.set_bitdepth(8);
    state.info_png.color.colortype = out_type;
    state.info_png.color.set_bitdepth(8);
    state.encode(pixels, pixels.len(), 1)
}

#[test]
fn bgr() {
    let png = encode(&[rgb::alt::BGR{r:1u8,g:2,b:3}], ColorType::BGR, ColorType::RGB).unwrap();
    let img = decode24(&png).unwrap();
    assert_eq!(img.buffer[0], rgb::RGB{r:1,g:2,b:3});

    let png = encode(&[rgb::alt::BGRA{r:1u8,g:2,b:3,a:111u8}], ColorType::BGRX, ColorType::RGB).unwrap();
    let img = decode32(&png).unwrap();
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
    let img2 = decode_memory(&png, ColorType::GREY, 8).unwrap();
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
    let img2 = decode24(&png).unwrap();

    assert_eq!(img1.buffer, img2.buffer);
}

#[test]
fn random() {
    let mut data = vec![0u8; 639*479*3];
    for (i, px) in data.iter_mut().enumerate() {
        *px = ((i ^ (13 + i * 17) ^ (i * 13) ^ (i/113 * 11)) >> 5) as u8;
    }

    let png = encode24(&data, 639, 479).unwrap();
    let img2 = decode24(&png).unwrap();

    use rgb::*;
    assert_eq!(data.as_rgb(), &img2.buffer[..]);
}

#[test]
fn bgra() {
    let png = encode(&[rgb::alt::BGRA{r:1u8,g:2,b:3,a:4u8}], ColorType::BGRA, ColorType::RGBA).unwrap();
    let img = decode32(&png).unwrap();
    assert_eq!(img.buffer[0], rgb::RGBA8{r:1,g:2,b:3,a:4u8});
}

#[test]
fn text_chunks() {
    let mut s = State::new();
    s.encoder.text_compression = 0;
    let longstr = "World 123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_";
    assert!(longstr.len() > 89);
    s.info_png_mut().add_text("Hello", longstr).unwrap();
    assert_eq!(1, s.info_png().text_keys_cstr().count());
    let data = s.encode(&[0], 1, 1).unwrap();

    assert!(data.windows(4).any(|w| w == b"tEXt"));

    let mut s = State::new();
    s.read_text_chunks(true);
    s.decode(data).unwrap();
    assert_eq!(1, s.info_png().text_keys_cstr().count());
}

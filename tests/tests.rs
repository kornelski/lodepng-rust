extern crate lodepng;

#[test]
fn text_chunks() {
    let mut s = lodepng::State::new();
    s.encoder.text_compression = 0;
    let longstr = "World 123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_";
    assert!(longstr.len() > 89);
    s.info_png_mut().add_text("Hello", longstr).unwrap();
    assert_eq!(1, s.info_png().text_keys_cstr().count());
    let data = s.encode(&[0], 1, 1).unwrap();

    assert!(data.windows(4).any(|w| w == b"tEXt"));

    let mut s = lodepng::State::new();
    s.read_text_chunks(true);
    s.decode(data).unwrap();
    assert_eq!(1, s.info_png().text_keys_cstr().count());
}

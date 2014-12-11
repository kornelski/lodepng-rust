extern crate lodepng;

fn main() {
    let path = &Path::new("write_test.png");

    let image : [u8, ..12] = [
                 255, 0, 0,
                 0, 255, 0,
                 0, 0, 255,
                 0, 99, 99];

    // encode_file takes the path to the image, a u8 array,
    // the width, the height, the color mode, and the bit depth
    match lodepng::encode_file(path, image.as_slice(), 2, 2, lodepng::LCT_RGB, 8) {
        Err(e) => panic!("failed to write png: {}", e),
        Ok(_) => (),
   }
}

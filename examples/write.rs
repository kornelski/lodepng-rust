extern crate lodepng;
use std::path::Path;

fn main() {
    let path = &Path::new("write_test.png");

    let image = [255, 0, 0,   0, 255, 0,
                 0, 0, 255,   0, 99, 99];

    // encode_file takes the path to the image, a u8 array,
    // the width, the height, the color mode, and the bit depth
    if let Err(e) = lodepng::encode_file(path, &image, 2, 2, lodepng::LCT_RGB, 8) {
        panic!("failed to write png: {}", e);
    }

    println!("Written to {}", path.display());
}

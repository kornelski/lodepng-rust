extern crate lodepng;
use std::path::Path;

fn main() {
    let from_path = "tests/test.png";

    // Since we're using decode24_file, we get an RGB bitmap
    let bitmap = match lodepng::decode24_file(from_path) {
        Ok(bitmap) => bitmap,
        Err(reason) => panic!("Could not load {}, because: {}", from_path, reason),
    };

    let to_path = &Path::new("write_test.png");

    let buffer = bitmap.buffer.as_ref();

    // Now we reencode it, using LCT_RGB since we used decode24_file
    if let Err(e) = lodepng::encode_file(to_path, buffer, bitmap.width, bitmap.height, lodepng::LCT_RGB, 8) {
        panic!("failed to write png: {}", e);
    }

    println!("Copied {} to {}", from_path, to_path.display());
}

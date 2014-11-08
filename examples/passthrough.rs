extern crate lodepng;

fn main() {
    let path = &Path::new("test.png");

    // Since we're using decode24_file, we get an RGB bitmap
    let bitmap = match lodepng::decode24_file(path) {
        Ok(bitmap) => bitmap,
        Err(reason) => panic!("Could not load {}, because: {}", path.display(), reason),
    };

    let path = &Path::new("write_test.png");

    let buffer = bitmap.buffer.as_slice();
    // Now we reencode it, using LCT_RGB since we used decode24_file
    match lodepng::encode_file(path, buffer, bitmap.width, bitmap.height, lodepng::LCT_RGB, 8) {
        Err(e) => panic!("failed to write png: {}", e),
        Ok(_) => (),
    }
}

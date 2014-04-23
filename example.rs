extern crate lodepng;

fn main() {
    let filename = "test.png";

    match lodepng::decode32_file(filename) {
        Ok(bitmap) => println!("Decoded image {} x {} and the first pixel's red value is {}",
                                bitmap.width, bitmap.height, bitmap.buffer.as_slice()[0]),
        Err(reason) => println!("Could not load {}, because: {}", filename, reason),
    }
}

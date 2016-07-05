extern crate lodepng;
use std::path::Path;

fn main() {
    let path = &Path::new("test.png");

    let mut state = lodepng::State::new();

    match state.decode_file(path) {
        Ok(image) => match image {
            lodepng::Image::RGBA(bitmap) => {
                println!("Decoded image {} x {} and the first pixel's value is {}",
                            bitmap.width, bitmap.height, bitmap.buffer.get(0).unwrap());
            },
            x => println!("Decoded some other image format {:?}", x),
        },
        Err(reason) => println!("Could not load {}, because: {}", path.display(), reason),
    }
}

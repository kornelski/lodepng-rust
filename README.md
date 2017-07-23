# [LodePNG](http://lodev.org/lodepng) bindings for [Rust](https://www.rust-lang.org)

LodePNG is a stand-alone PNG image decoder and encoder (does *not* require zlib nor libpng) written in C.

This package allows easy reading and writing of PNG files without any system dependencies.

The easiest way to use LodePNG is to include the [lodepng crate](https://crates.io/crates/lodepng).
To do so, add this to your `Cargo.toml`:

```toml
[dependencies]
lodepng = "1.1.3"
```

## API

See [API documentation](https://pornel.github.io/lodepng-rust/lodepng/) for details. The API mimics lodepng, so if something is unclear, [see the original lodepng.h](https://raw.githubusercontent.com/lvandeve/lodepng/master/lodepng.h).

### Loading image example

```rust
let image = lodepng::decode32_file("in.png")?;
```

returns image of type `lodepng::Bitmap<lodepng::RGBA<u8>>` with fields `.width`, `.height`, and `.buffer`. The buffer is a `CVec`. To get a regular slice (`&[RGBA]`), use `image.buffer.as_ref()`.

The RGB/RGBA pixel types are from the [RGB crate](https://crates.io/crates/rgb), which you can import separately to use the same pixel struct throughout the program, without casting. But if you want to read the image buffer as bunch of raw bytes, ignoring the RGB(A) types, run `cargo add rgb` and use:

```rust
extern crate rgb;
use rgb::*;
…
let bytes: &[u8] = image.buffer.as_ref().as_bytes();
```

### Saving image example

```rust
lodepng::encode32_file("out.png", &buffer, width, height)
```

### Advanced

```rust
let mut state = lodepng::State::new();
state.remember_unknown_chunks(true);

match state.decode("in.png") {
    Ok(lodepng::Image::RGB(image)) => {…}
    Ok(lodepng::Image::RGBA(image)) => {…}
    Ok(lodepng::Image::RGBA16(image)) => {…}
    Ok(lodepng::Image::Gray(image)) => {…}
    Ok(_) => {…}
    Err(err) => {…}
}

for chunk in state.info_png().unknown_chunks() {
    println!("{:?} = {:?}", chunk.name(), chunk.data());
}

// Color profile (to be used with e.g. LCMS2)
let icc_data = state.info_png().get_icc();
```

### Requirements

* At build time: a C compiler
* At run time: libc

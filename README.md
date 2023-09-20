# [Rust](https://www.rust-lang.org) rewrite of [LodePNG](https://lodev.org/lodepng)

This is a pure Rust PNG image decoder and encoder. Allows easy reading and writing of PNG files without any system dependencies.

To use the [lodepng crate](https://lib.rs/crates/lodepng), add to your `Cargo.toml`:

```toml
[dependencies]
lodepng = "3.8"
```

See [the API reference](https://docs.rs/lodepng/) for details. Requires Rust 1.64 or later.

### Loading image example

[There are more examples in the `examples/` dir](https://github.com/kornelski/lodepng-rust/tree/main/examples).

```rust
let image = lodepng::decode32_file("in.png")?;
```

returns image of type `lodepng::Bitmap<lodepng::RGBA<u8>>` with fields `.width`, `.height`, and `.buffer` (the buffer is a `Vec`).

The RGB/RGBA pixel types are from the [RGB crate](https://lib.rs/crates/rgb), which you can import separately to use the same pixel struct throughout the program, without casting. But if you want to read the image buffer as bunch of raw bytes, ignoring the RGB(A) pixel structure, use:

```toml
[dependencies]
rgb = "0.8"
```

```rust
use rgb::*;
…
let bytes: &[u8] = image.buffer.as_bytes();
```

### Saving image example

```rust
lodepng::encode32_file("out.png", &buffer, width, height)?
```

The RGBA buffer can be a slice of any type, as long as it has 4 bytes per element (e.g. `struct RGBA` or `[u8; 4]`) and implements the [`Pod`](https://docs.rs/bytemuck) trait.

### Advanced

```rust
let mut state = lodepng::Decoder::new();
state.remember_unknown_chunks(true);

match state.decode("in.png") {
    Ok(lodepng::Image::RGB(image)) => {…}
    Ok(lodepng::Image::RGBA(image)) => {…}
    Ok(lodepng::Image::RGBA16(image)) => {…}
    Ok(lodepng::Image::Grey(image)) => {…}
    Ok(_) => {…}
    Err(err) => {…}
}

for chunk in state.info_png().unknown_chunks(ChunkPosition::IHDR) {
    println!("{:?} = {:?}", chunk.name(), chunk.data());
}

// Color profile (to be used with e.g. LCMS2)
let icc_data = state.get_icc();
```

See [load_image](https://lib.rs/crates/load_image) crate for an example how to use lodepng with [color profiles](https://lib.rs/lcms2).

## Upgrading from 2.x

* C FFI still exists, but is no longer ABI-compatible with the original C lodepng due to layout changes in structs.
* Structs use `bool` where appropriate instead of 0/1 `int`.
* Custom zlib callbacks use `io::Write` instead of `malloc`-ed buffers (remember to use `write_all`, not `write`!)
* `ffi::Error` has been renamed to `ffi::ErrorCode`.
* Compression level is set via `set_level()` function instead of individual `CompressConfig` fields.

## Upgrading from 1.x

* `CVec` has been replaced with a regular `Vec`. Delete extra `.as_ref()` that the compiler may complain about.
* `LCT_*` constants have been changed to `ColorType::*`.
* `Chunk`/`Chunks` renamed to `ChunkRef`/`ChunksIter`.
* `auto_convert` is a boolean.
* `bitdepth` has a getter/setter.
* There is no C any more!

## It's a ship of Theseus

This codebase has been derived from [the C LodePNG](https://lodev.org/lodepng/) by Lode Vandevenne. It has been converted to Rust using [Citrus C to Rust converter](https://gitlab.com/citrus-rs/citrus), and then significanlty refactored to be more idiomatic Rust. As a result, it's now a different library, with some of the original API.

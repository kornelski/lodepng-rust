#[LodePNG](http://lodev.org/lodepng/) bindings for [Rust](http://www.rust-lang.org/)

LodePNG is a stand-alone PNG image decoder and encoder (does *not* require zlib or libpng).

This package allows easy reading and writing of PNG files without any system dependencies.

To build the `lodepng` crate:

    cargo build

It will produce `liblodepng-….rlib`. For documentation of the structures and functions, [see the original lodepng.h](http://lpi.googlecode.com/svn/trunk/lodepng.h).


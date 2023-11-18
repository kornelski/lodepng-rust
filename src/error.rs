use std::collections::TryReserveError;
use crate::ffi::ErrorCode;
use std::error;
use std::fmt;
use std::io;
use std::num::NonZeroU32;

#[derive(Copy, Clone)]
pub struct Error(NonZeroU32);

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl ErrorCode {
    /// Returns an English description of the numerical error code.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        let s = self.c_description();
        let s = s.get(..s.len() - 1).unwrap_or_default(); // trim \0
        std::str::from_utf8(s).unwrap_or("")
    }

    /// Helper function for the library
    #[inline]
    pub fn to_result(self) -> Result<(), Error> {
        match NonZeroU32::new(self.0) {
            None => Ok(()),
            Some(err) => Err(Error(err)),
        }
    }
}

impl Error {
    /// Panics if the code is 0
    #[cold]
    #[must_use]
    pub const fn new(code: u32) -> Self {
        Self(match NonZeroU32::new(code) { Some(s) => s, None => panic!() })
    }
}

impl From<ErrorCode> for Result<(), Error> {
    #[cold]
    fn from(err: ErrorCode) -> Self {
        err.to_result()
    }
}

impl From<Error> for ErrorCode {
    #[inline(always)]
    fn from(err: Error) -> Self {
        ErrorCode(err.0.get())
    }
}

impl fmt::Debug for Error {
    #[cold]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", ErrorCode(self.0.get()).as_str(), self.0)
    }
}

impl fmt::Debug for ErrorCode {
    #[cold]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.as_str(), self.0)
    }
}

impl fmt::Display for Error {
    #[cold]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(ErrorCode(self.0.get()).as_str())
    }
}

impl error::Error for Error {}

#[doc(hidden)]
impl std::convert::From<io::Error> for Error {
    #[cold]
    fn from(err: io::Error) -> Error {
        match err.kind() {
            io::ErrorKind::NotFound | io::ErrorKind::UnexpectedEof => Error::new(78),
            _ => Error::new(79),
        }
    }
}

/*
This returns the description of a numerical error code in English. This is also
the documentation of all the error codes.
*/
impl ErrorCode {
    #[cold]
    #[must_use]
    pub fn c_description(&self) -> &'static [u8] {
        match self.0 {
            0 => "no error, everything went ok\0",
            1 => "nothing done yet\0",

            /*the Encoder/Decoder has done nothing yet, error checking makes no sense yet*/
            10 => "end of input memory reached without huffman end code\0",

            /*while huffman decoding*/
            11 => "error in code tree made it jump outside of huffman tree\0",

            /*while huffman decoding*/
            13 | 14 | 15 => "problem while processing dynamic deflate block\0",
            16 => "unexisting code while processing dynamic deflate block\0",
            18 => "invalid distance code while inflating\0",
            17 | 19 | 22 => "end of out buffer memory reached while inflating\0",
            20 => "invalid deflate block BTYPE encountered while decoding\0",
            21 => "NLEN is not ones complement of LEN in a deflate block\0",

            /*end of out buffer memory reached while inflating:
                This can happen if the inflated deflate data is longer than the amount of bytes required to fill up
                all the pixels of the image, given the color depth and image dimensions. Something that doesn't
                happen in a normal, well encoded, PNG image.*/
            23 => "end of in buffer memory reached while inflating\0",
            24 => "invalid FCHECK in zlib header\0",
            25 => "invalid compression method in zlib header\0",
            26 => "FDICT encountered in zlib header while it\'s not used for PNG\0",
            27 => "PNG file is smaller than a PNG header\0",
            /*Checks the magic file header, the first 8 bytes of the PNG file*/
            28 => "incorrect PNG signature, it\'s no PNG or corrupted\0",
            29 => "first chunk is not the header chunk\0",
            30 => "chunk length too large, chunk broken off at end of file\0",
            31 => "illegal PNG color type or bpp\0",
            32 => "illegal PNG compression method\0",
            33 => "illegal PNG filter method\0",
            34 => "illegal PNG interlace method\0",
            35 => "chunk length of a chunk is too large or the chunk too small\0",
            36 => "illegal PNG filter type encountered\0",
            37 => "illegal bit depth for this color type given\0",
            38 => "the palette is too big\0",
            /*more than 256 colors*/
            39 => "more palette alpha values given in tRNS chunk than there are colors in the palette\0",
            40 => "tRNS chunk has wrong size for greyscale image\0",
            41 => "tRNS chunk has wrong size for RGB image\0",
            42 => "tRNS chunk appeared while it was not allowed for this color type\0",
            43 => "bKGD chunk has wrong size for palette image\0",
            44 => "bKGD chunk has wrong size for greyscale image\0",
            45 => "bKGD chunk has wrong size for RGB image\0",
            48 => "empty input buffer given to decoder. Maybe caused by non-existing file?\0",
            49 | 50 => "jumped past memory while generating dynamic huffman tree\0",
            51 => "jumped past memory while inflating huffman block\0",
            52 => "jumped past memory while inflating\0",
            53 => "size of zlib data too small\0",
            54 => "repeat symbol in tree while there was no value symbol yet\0",

            /*jumped past tree while generating huffman tree, this could be when the
               tree will have more leaves than symbols after generating it out of the
               given lenghts. They call this an oversubscribed dynamic bit lengths tree in zlib.*/
            55 => "jumped past tree while generating huffman tree\0",
            56 => "given output image colortype or bitdepth not supported for color conversion\0",
            57 => "invalid CRC encountered (checking CRC can be disabled)\0",
            58 => "invalid ADLER32 encountered (checking ADLER32 can be disabled)\0",
            59 => "requested color conversion not supported\0",
            60 => "invalid window size given in the settings of the encoder (must be 0-32768)\0",
            61 => "invalid BTYPE given in the settings of the encoder (only 0, 1 and 2 are allowed)\0",

            /*LodePNG leaves the choice of RGB to greyscale conversion formula to the user.*/
            62 => "conversion from color to greyscale not supported\0",
            63 => "length of a chunk too long, max allowed for PNG is 2147483647 bytes per chunk\0",

            /*(2^31-1)*/
            /*this would result in the inability of a deflated block to ever contain an end code. It must be at least 1.*/
            64 => "the length of the END symbol 256 in the Huffman tree is 0\0",
            66 => "the length of a text chunk keyword given to the encoder is longer than the maximum of 79 bytes\0",
            67 => "the length of a text chunk keyword given to the encoder is smaller than the minimum of 1 byte\0",
            68 => "tried to encode a PLTE chunk with a palette that has less than 1 or more than 256 colors\0",
            69 => "unknown chunk type with \'critical\' flag encountered by the decoder\0",
            71 => "unexisting interlace mode given to encoder (must be 0 or 1)\0",
            72 => "while decoding, unexisting compression method encountering in zTXt or iTXt chunk (it must be 0)\0",
            73 => "invalid tIME chunk size\0",
            74 => "invalid pHYs chunk size\0",
            /*length could be wrong, or data chopped off*/
            75 => "no null termination char found while decoding text chunk\0",
            76 => "iTXt chunk too short to contain required bytes\0",
            77 => "integer overflow in buffer size\0",
            78 => "failed to open file for reading\0",

            /*file doesn't exist or couldn't be opened for reading*/
            79 => "failed to open file for writing\0",
            80 => "tried creating a tree of 0 symbols\0",
            81 => "lazy matching at pos 0 is impossible\0",
            82 => "color conversion to palette requested while a color isn\'t in palette\0",
            83 => "memory allocation failed\0",
            84 => "given image too small to contain all pixels to be encoded\0",
            86 => "impossible offset in lz77 encoding (internal bug)\0",
            87 => "must provide custom zlib function pointer if LODEPNG_COMPILE_ZLIB is not defined\0",
            88 => "invalid filter strategy given for EncoderSettings.filter_strategy\0",
            89 => "text chunk keyword too short or long: must have size 1-79\0",

            /*the windowsize in the CompressSettings. Requiring POT(==> & instead of %) makes encoding 12% faster.*/
            90 => "windowsize must be a power of two\0",
            91 => "invalid decompressed idat size\0",
            92 => "too many pixels, not supported\0",
            93 => "zero width or height is invalid\0",
            94 => "header chunk must have a size of 13 bytes\0",
            _ => "unknown error code\0",
        }.as_bytes()
    }
}

impl From<TryReserveError> for Error {
    #[cold]
    fn from(_: TryReserveError) -> Self {
        Self(NonZeroU32::new(83).unwrap())
    }
}

#[cfg(feature = "deprecated_back_compat_error_type")]
/// Back compat only
impl From<fallible_collections::TryReserveError> for Error {
    #[cold]
    fn from(_: fallible_collections::TryReserveError) -> Self {
        Self(NonZeroU32::new(83).unwrap())
    }
}

#[test]
fn error_str() {
    assert_eq!(ErrorCode(83).as_str(), "memory allocation failed");
}

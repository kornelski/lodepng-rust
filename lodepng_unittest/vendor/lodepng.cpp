/*
LodePNG version 20161127

Copyright (c) 2005-2016 Lode Vandevenne

This software is provided 'as-is', without any express or implied
warranty. In no event will the authors be held liable for any damages
arising from the use of this software.

Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it
freely, subject to the following restrictions:

    1. The origin of this software must not be misrepresented; you must not
    claim that you wrote the original software. If you use this software
    in a product, an acknowledgment in the product documentation would be
    appreciated but is not required.

    2. Altered source versions must be plainly marked as such, and must not be
    misrepresented as being the original software.

    3. This notice may not be removed or altered from any source
    distribution.
*/

/*
The manual and changelog are in the header file "lodepng.h"
Rename this file to lodepng.cpp to use it for C++, or to lodepng.c to use it for C.
*/

#include "lodepng.h"

#include <limits.h>
#include <stdio.h>
#include <stdlib.h>

#if defined(_MSC_VER) && (_MSC_VER >= 1310) /*Visual Studio: A few warning types are not desired here.*/
#pragma warning( disable : 4244 ) /*implicit conversions: not warned by gcc -Wall -Wextra and requires too much casts*/
#pragma warning( disable : 4996 ) /*VS does not like fopen, but fopen_s is not standard C so unusable here*/
#endif /*_MSC_VER */

const char* LODEPNG_VERSION_STRING = "20161127";

/*
This source file is built up in the following large parts. The code sections
with the "LODEPNG_COMPILE_" #defines divide this up further in an intermixed way.
-Tools for C and common code for PNG and Zlib
-C Code for Zlib (huffman, deflate, ...)
-C Code for PNG (file format chunks, adam7, PNG filters, color conversions, ...)
-The C++ wrapper around all of the above
*/

/*The malloc, realloc and free functions defined here with "lodepng_" in front
of the name, so that you can easily change them to others related to your
platform if needed. Everything else in the code calls these. Pass
-DLODEPNG_NO_COMPILE_ALLOCATORS to the compiler, or comment out
#define LODEPNG_COMPILE_ALLOCATORS in the header, to disable the ones here and
define them in your own project's source files without needing to change
lodepng source code. Don't forget to remove "static" if you copypaste them
from here.*/

extern "C" void* lodepng_malloc(size_t size);

extern "C" void* lodepng_realloc(void* ptr, size_t new_size);

extern "C" void lodepng_free(void* ptr);

/* returns negative value on error. This should be pure C compatible, so no fstat. */
extern "C" long lodepng_filesize(const char* filename);

/* load file into buffer that already has the correct allocated size. Returns error code.*/
extern "C" unsigned lodepng_buffer_file(unsigned char* out, size_t size, const char* filename);


/*write given buffer to the file, overwriting the file, it doesn't append to it.*/
extern "C" unsigned lodepng_save_file(const unsigned char* buffer, size_t buffersize, const char* filename);

/* /////////////////////////////////////////////////////////////////////////// */

extern "C" unsigned zlib_decompress(unsigned char** out, size_t* outsize, const unsigned char* in,
                                size_t insize, const LodePNGDecompressSettings* settings);


/* compress using the default or custom zlib function */
extern "C" unsigned zlib_compress(unsigned char** out, size_t* outsize, const unsigned char* in,
                              size_t insize, const LodePNGCompressSettings* settings);


extern "C" size_t lodepng_get_raw_size_lct(unsigned w, unsigned h, LodePNGColorType colortype, unsigned bitdepth);

extern "C" unsigned lodepng_decode(unsigned char** out, unsigned* w, unsigned* h,
                        LodePNGState* state,
                        const unsigned char* in, size_t insize);

extern "C" unsigned lodepng_decode_memory(unsigned char** out, unsigned* w, unsigned* h, const unsigned char* in,
                               size_t insize, LodePNGColorType colortype, unsigned bitdepth);


extern "C" void lodepng_state_init(LodePNGState* state);

extern "C" void lodepng_state_cleanup(LodePNGState* state);

extern "C" void lodepng_state_copy(LodePNGState* dest, const LodePNGState* source);


/* ////////////////////////////////////////////////////////////////////////// */
/* / PNG Encoder                                                            / */
/* ////////////////////////////////////////////////////////////////////////// */

extern "C" unsigned lodepng_encode(unsigned char** out, size_t* outsize,
                        const unsigned char* image, unsigned w, unsigned h,
                        LodePNGState* state);

extern "C" unsigned lodepng_encode_memory(unsigned char** out, size_t* outsize, const unsigned char* image,
                               unsigned w, unsigned h, LodePNGColorType colortype, unsigned bitdepth);


/* ////////////////////////////////////////////////////////////////////////// */
/* ////////////////////////////////////////////////////////////////////////// */
/* // C++ Wrapper                                                          // */
/* ////////////////////////////////////////////////////////////////////////// */
/* ////////////////////////////////////////////////////////////////////////// */

#ifdef __cplusplus
namespace lodepng
{

  unsigned load_file(std::vector<unsigned char>& buffer, const std::string& filename)
  {
    long size = lodepng_filesize(filename.c_str());
    if(size < 0) return 78;
    buffer.resize((size_t)size);
    return size == 0 ? 0 : lodepng_buffer_file(&buffer[0], (size_t)size, filename.c_str());
  }

  /*write given buffer to the file, overwriting the file, it doesn't append to it.*/
  unsigned save_file(const std::vector<unsigned char>& buffer, const std::string& filename)
  {
    return lodepng_save_file(buffer.empty() ? 0 : &buffer[0], buffer.size(), filename.c_str());
  }

  unsigned decompress(std::vector<unsigned char>& out, const unsigned char* in, size_t insize,
                      const LodePNGDecompressSettings& settings)
  {
    unsigned char* buffer = 0;
    size_t buffersize = 0;
    unsigned error = zlib_decompress(&buffer, &buffersize, in, insize, &settings);
    if(buffer)
    {
      out.insert(out.end(), &buffer[0], &buffer[buffersize]);
      lodepng_free(buffer);
    }
    return error;
  }

  unsigned decompress(std::vector<unsigned char>& out, const std::vector<unsigned char>& in,
                      const LodePNGDecompressSettings& settings)
  {
    return decompress(out, in.empty() ? 0 : &in[0], in.size(), settings);
  }

  unsigned compress(std::vector<unsigned char>& out, const unsigned char* in, size_t insize,
                    const LodePNGCompressSettings& settings)
  {
    unsigned char* buffer = 0;
    size_t buffersize = 0;
    unsigned error = zlib_compress(&buffer, &buffersize, in, insize, &settings);
    if(buffer)
    {
      out.insert(out.end(), &buffer[0], &buffer[buffersize]);
      lodepng_free(buffer);
    }
    return error;
  }

  unsigned compress(std::vector<unsigned char>& out, const std::vector<unsigned char>& in,
                    const LodePNGCompressSettings& settings)
  {
    return compress(out, in.empty() ? 0 : &in[0], in.size(), settings);
  }



  State::State()
  {
    lodepng_state_init(this);
  }

  State::State(const State& other)
  {
    lodepng_state_init(this);
    lodepng_state_copy(this, &other);
  }

  State::~State()
  {
    lodepng_state_cleanup(this);
  }

  State& State::operator=(const State& other)
  {
    lodepng_state_copy(this, &other);
    return *this;
  }


  unsigned decode(std::vector<unsigned char>& out, unsigned& w, unsigned& h, const unsigned char* in,
                  size_t insize, LodePNGColorType colortype, unsigned bitdepth)
  {
    unsigned char* buffer;
    unsigned error = lodepng_decode_memory(&buffer, &w, &h, in, insize, colortype, bitdepth);
    if(buffer && !error)
    {
      State state;
      state.info_raw.colortype = colortype;
      state.info_raw.bitdepth = bitdepth;
      size_t buffersize = lodepng_get_raw_size(w, h, &state.info_raw);
      out.insert(out.end(), &buffer[0], &buffer[buffersize]);
      lodepng_free(buffer);
    }
    return error;
  }

  unsigned decode(std::vector<unsigned char>& out, unsigned& w, unsigned& h,
                  const std::vector<unsigned char>& in, LodePNGColorType colortype, unsigned bitdepth)
  {
    return decode(out, w, h, in.empty() ? 0 : &in[0], (unsigned)in.size(), colortype, bitdepth);
  }

  unsigned decode(std::vector<unsigned char>& out, unsigned& w, unsigned& h,
                  State& state,
                  const unsigned char* in, size_t insize)
  {
    unsigned char* buffer = NULL;
    unsigned error = lodepng_decode(&buffer, &w, &h, &state, in, insize);
    if(buffer && !error)
    {
      size_t buffersize = lodepng_get_raw_size(w, h, &state.info_raw);
      out.insert(out.end(), &buffer[0], &buffer[buffersize]);
    }
    lodepng_free(buffer);
    return error;
  }

  unsigned decode(std::vector<unsigned char>& out, unsigned& w, unsigned& h,
                  State& state,
                  const std::vector<unsigned char>& in)
  {
    return decode(out, w, h, state, in.empty() ? 0 : &in[0], in.size());
  }

  unsigned decode(std::vector<unsigned char>& out, unsigned& w, unsigned& h, const std::string& filename,
                  LodePNGColorType colortype, unsigned bitdepth)
  {
    std::vector<unsigned char> buffer;
    unsigned error = load_file(buffer, filename);
    if(error) return error;
    return decode(out, w, h, buffer, colortype, bitdepth);
  }

  unsigned encode(std::vector<unsigned char>& out, const unsigned char* in, unsigned w, unsigned h,
                  LodePNGColorType colortype, unsigned bitdepth)
  {
    unsigned char* buffer;
    size_t buffersize;
    unsigned error = lodepng_encode_memory(&buffer, &buffersize, in, w, h, colortype, bitdepth);
    if(buffer)
    {
      out.insert(out.end(), &buffer[0], &buffer[buffersize]);
      lodepng_free(buffer);
    }
    return error;
  }

  unsigned encode(std::vector<unsigned char>& out,
                  const std::vector<unsigned char>& in, unsigned w, unsigned h,
                  LodePNGColorType colortype, unsigned bitdepth)
  {
    if(lodepng_get_raw_size_lct(w, h, colortype, bitdepth) > in.size()) return 84;
    return encode(out, in.empty() ? 0 : &in[0], w, h, colortype, bitdepth);
  }

  unsigned encode(std::vector<unsigned char>& out,
                  const unsigned char* in, unsigned w, unsigned h,
                  State& state)
  {
    unsigned char* buffer;
    size_t buffersize;
    unsigned error = lodepng_encode(&buffer, &buffersize, in, w, h, &state);
    if(buffer)
    {
      out.insert(out.end(), &buffer[0], &buffer[buffersize]);
      lodepng_free(buffer);
    }
    return error;
  }

  unsigned encode(std::vector<unsigned char>& out,
                  const std::vector<unsigned char>& in, unsigned w, unsigned h,
                  State& state)
  {
    if(lodepng_get_raw_size(w, h, &state.info_raw) > in.size()) return 84;
    return encode(out, in.empty() ? 0 : &in[0], w, h, state);
  }

  unsigned encode(const std::string& filename,
                  const unsigned char* in, unsigned w, unsigned h,
                  LodePNGColorType colortype, unsigned bitdepth)
  {
    std::vector<unsigned char> buffer;
    unsigned error = encode(buffer, in, w, h, colortype, bitdepth);
    if(!error) error = save_file(buffer, filename);
    return error;
  }

  unsigned encode(const std::string& filename,
                  const std::vector<unsigned char>& in, unsigned w, unsigned h,
                  LodePNGColorType colortype, unsigned bitdepth)
  {
    if(lodepng_get_raw_size_lct(w, h, colortype, bitdepth) > in.size()) return 84;
    return encode(filename, in.empty() ? 0 : &in[0], w, h, colortype, bitdepth);
  }
} /* namespace lodepng */
#endif

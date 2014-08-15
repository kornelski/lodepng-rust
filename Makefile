RUSTC ?= rustc

RUSTLIBSRC=lodepng.rs
RUSTLIB=$(shell $(RUSTC) --print-file-name $(RUSTLIBSRC))
CFLAGS ?= -O3 -fPIC

all: example crate

crate: $(RUSTLIB)

$(RUSTLIB): $(RUSTLIBSRC) liblodepng.a
	$(RUSTC) -L . $<

liblodepng.a: liblodepng.o
	$(AR) $(ARFLAGS) $@ $^

liblodepng.o: lodepng.c
	$(CC) $(CFLAGS) -c -o $@ $^

lodepng.c: lodepng.h
	curl -L http://lpi.googlecode.com/svn/trunk/lodepng.cpp -o $@

lodepng.h:
	curl -L http://lpi.googlecode.com/svn/trunk/lodepng.h -o $@

example: $(RUSTLIB) example.rs
	$(RUSTC) -o $@ -L . example.rs
	@echo Run ./example

clean:
	rm -rf $(RUSTLIB) *.o example

distclean: clean
	rm -rf lodepng.[ch]

.PHONY: all crate clean distclean

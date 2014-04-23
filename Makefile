RUSTC ?= rustc

RUSTLIBSRC=lodepng.rs
RUSTLIB=$(shell $(RUSTC) --crate-file-name $(RUSTLIBSRC))
CFLAGS ?= -O3

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
	$(RUSTC) -L . example.rs
	@echo run with: ./example

clean:
	rm -rf $(RUSTLIB) *.o

distclean: clean
	rm -rf lodepng.[ch]

.PHONY: all crate clean distclean

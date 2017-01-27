CFLAGS ?= -O3 -fPIC
OUT_DIR ?= .
VENDOR_DIR ?= vendor
LIB = $(OUT_DIR)/liblodepng.a
OBJ = $(OUT_DIR)/liblodepng.o
SRC = $(VENDOR_DIR)/lodepng.c
HEADER = $(VENDOR_DIR)/lodepng.h

$(LIB): $(OBJ)
	$(AR) $(ARFLAGS) $@ $^

$(OBJ): $(SRC)
	$(CC) $(CFLAGS) -c -o $@ $^

$(SRC): $(HEADER)

$(SRC):
	curl -L https://raw.githubusercontent.com/lvandeve/lodepng/master/lodepng.cpp -o $@

$(HEADER):
	curl -L https://raw.githubusercontent.com/lvandeve/lodepng/master/lodepng.h -o $@

doc: src/lib.rs
	rustdoc --html-before-content doc/_header.html -L target/debug/deps/ $^

clean:
	-rm -f -- $(SRC) $(HEADER) $(OBJ) $(LIB)

.PHONY: doc clean


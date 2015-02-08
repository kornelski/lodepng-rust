CFLAGS ?= -O3 -fPIC
OUT_DIR ?= .
LIB = $(OUT_DIR)/liblodepng.a
OBJ = $(OUT_DIR)/liblodepng.o
SRC = $(OUT_DIR)/lodepng.c
HEADER = $(OUT_DIR)/lodepng.h

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
	rustdoc --html-before-content doc/_header.html -L target/deps/ $^

.PHONY: doc


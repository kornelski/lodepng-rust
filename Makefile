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
	curl -L http://lpi.googlecode.com/svn/trunk/lodepng.cpp -o $@

$(HEADER):
	curl -L http://lpi.googlecode.com/svn/trunk/lodepng.h -o $@

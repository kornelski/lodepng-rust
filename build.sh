#!/bin/sh

if type mingw32-make 2>/dev/null; then
    CC=gcc mingw32-make
else
    make
fi

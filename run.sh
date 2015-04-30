#!/bin/sh
cd ~/repos/RustJni
export LD_LIBRARY_PATH="/usr/lib/jvm/java/jre/lib/amd64/server/" \
       RUST_BACKTRACE=1
exec ./main

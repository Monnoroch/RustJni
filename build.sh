#!/bin/sh -e
runpath="$JAVA_HOME/jre/lib/amd64/server/"
set -e
export LD_RUN_PATH="$runpath"
rustc lib/lib.rs -g
rustc tests/main.rs -L . -g -L "$runpath"

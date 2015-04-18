#!/bin/sh
rustc lib/lib.rs && rustc tests/main.rs -L . -L "$JAVA_HOME/amd64/server/"


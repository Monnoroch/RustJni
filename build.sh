#!/bin/sh
rustc lib/lib.rs && rustc tests/main.rs -L . -L /usr/lib/jvm/java-7-openjdk-amd64/jre/lib/amd64/jamvm/


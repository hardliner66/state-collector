#!/bin/bash

set -e

mkdir -p tmp
rm -rf tmp/*

cargo build --release

BIN=./target/release/state-collector
UNPACK=./target/release/sc-unpack

for type in basic binary json rsn; do
    $BIN $type -o tmp/$type.sc
    $UNPACK tmp/$type.sc
done

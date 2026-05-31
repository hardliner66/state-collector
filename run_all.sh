#!/bin/bash

mkdir -p tmp
rm -rf tmp/*

for type in basic binary json rsn; do
    cargo run --release -q -- $type -o tmp/$type.sc
    cargo run --release -q --bin sc-unpack -- tmp/$type.sc
done

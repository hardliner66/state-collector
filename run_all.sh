#!/bin/bash

mkdir -p tmp

for file in examples/*.rn; do
    filename=$(basename -- "$file")
    filename="${filename%.*}"
    cargo run --release -q -- examples/$filename.rn -o tmp/$filename.sc
    cargo run --release -q --bin sc-unpack -- tmp/$filename.sc
done

#!/bin/sh

set -e

objcopy --input-target=ihex --output-target=binary $1 $1.bin
dfu-util -a "@Internal Flash  /0x08000000/04*016Kg,01*064Kg,03*128Kg" -s 0x08000000 -D $1.bin

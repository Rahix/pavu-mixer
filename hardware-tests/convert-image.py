#!/usr/bin/env python3
import argparse
import os
from PIL import Image


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("image")
    parser.add_argument("output")
    args = parser.parse_args()

    im = Image.open(args.image)
    assert im.size[0] == 240
    assert im.size[1] == 240

    with open(args.output, "wb") as f:
        for x in range(im.size[1]):
            for y in range(im.size[0]):
                px = im.getpixel((y, x))

                px565 = ((px[0] >> 3) << 11) | ((px[1] >> 2) << 5) | (px[2] >> 3)
                byte_high = (px565 & 0xFF00) >> 8
                byte_low = px565 & 0xFF

                f.write(bytes([byte_high, byte_low]))


if __name__ == "__main__":
    main()

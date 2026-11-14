"""Scales the city art behind the HUD mockups into build/city_<realm>.jpg (1920 x 1080).

Each realm has its own painted city in ../../../images/references/ (1673 x 940,
16:9 to within a pixel). Needs Pillow.
"""
import pathlib

from PIL import Image

HERE = pathlib.Path(__file__).parent
IMAGES = HERE / "../../../images"
BUILD = HERE / "build"

CITIES = {
    "dailong": "Ancient Vietnamese Riverside Temple Town.jpg",
    "kambara": "Angkor Temple City in the Jungle.jpg",
    "tampadesa": "Golden Temple City by the River.jpg",
}


def cover(img, w, h):
    """Crops img to w:h about its centre, then scales it to w x h."""
    sw, sh = img.size
    if sw * h > sh * w:
        cw = round(sh * w / h)
        img = img.crop(((sw - cw) // 2, 0, (sw - cw) // 2 + cw, sh))
    else:
        ch = round(sw * h / w)
        img = img.crop((0, (sh - ch) // 2, sw, (sh - ch) // 2 + ch))
    return img.resize((w, h), Image.LANCZOS)


def main():
    BUILD.mkdir(exist_ok=True)
    for realm, name in CITIES.items():
        city = Image.open(IMAGES / "references" / name).convert("RGB")
        cover(city, 1920, 1080).save(BUILD / f"city_{realm}.jpg", quality=88)
        print("wrote", BUILD / f"city_{realm}.jpg")


if __name__ == "__main__":
    main()

"""Crops the city art behind the HUD mockups into build/city_<realm>.jpg (1920 x 1080).

Đại Long uses the April 2026 prototype screenshot; Kambara and Tampadesa use
the reference mocks, scaled up from 800 px wide. Needs Pillow.
"""
import pathlib

from PIL import Image, ImageFilter

HERE = pathlib.Path(__file__).parent
IMAGES = HERE / "../../../images"
BUILD = HERE / "build"


def save(img, name):
    BUILD.mkdir(exist_ok=True)
    img.resize((1920, 1080), Image.LANCZOS).save(BUILD / name, quality=88)
    print("wrote", BUILD / name)


def main():
    proto = Image.open(IMAGES / "prototypes/prototype_apr_2026_3.jpg").convert("RGB")
    save(proto.crop((330, 100, 2640, 1400)), "city_dailong.jpg")
    for realm, name in [("kambara", "mock_khr.jpg"), ("tampadesa", "mock_burma.jpg")]:
        mock = Image.open(IMAGES / "references" / name).convert("RGB")
        city = mock.crop((0, 110, 655, 478)).resize((1310, 736), Image.NEAREST).filter(ImageFilter.SMOOTH_MORE)
        save(city, f"city_{realm}.jpg")


if __name__ == "__main__":
    main()

"""Downloads the mockups' Google Fonts and writes build/fonts.css with them embedded.

Only the latin, latin-ext and vietnamese subsets are kept: Đại Long's names
need the Vietnamese diacritics on every realm's screens.
"""
import pathlib
import re
import subprocess

FAMILIES = [
    "Cinzel:wght@500;700", "Cinzel+Decorative:wght@700", "Marcellus",
    "Playfair+Display:wght@600;800", "Cormorant+Garamond:wght@500;700",
    "Rozha+One", "Alegreya:wght@500;700", "Alegreya+SC:wght@700",
    "Philosopher:wght@700", "Spectral+SC:wght@600",
]
KEEP = {"latin", "latin-ext", "vietnamese"}
UA = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/120 Safari/537.36"
BUILD = pathlib.Path(__file__).parent / "build"
CACHE = BUILD / "fonts"


def curl(url, out):
    subprocess.run(["curl", "-sSf", "-A", UA, "-o", str(out), url], check=True)


def main():
    import base64
    CACHE.mkdir(parents=True, exist_ok=True)
    faces = []
    for fam in FAMILIES:
        css = CACHE / (fam.split(":")[0] + ".css")
        if not css.exists():
            curl(f"https://fonts.googleapis.com/css2?family={fam}&display=swap", css)
        for m in re.finditer(r"/\* ([a-z-]+) \*/\s*@font-face \{(.*?)\}", css.read_text(), re.S):
            subset, body = m.group(1), m.group(2)
            if subset not in KEEP:
                continue
            url = re.search(r"url\((.*?)\)", body).group(1)
            woff = CACHE / url.split("/")[-1]
            if not woff.exists():
                curl(url, woff)
            data = base64.b64encode(woff.read_bytes()).decode()
            faces.append("@font-face {" + body.replace(f"url({url})", f"url(data:font/woff2;base64,{data})") + "}")
    (BUILD / "fonts.css").write_text("\n".join(faces))
    print(f"{len(faces)} font faces -> {BUILD / 'fonts.css'}")


if __name__ == "__main__":
    main()

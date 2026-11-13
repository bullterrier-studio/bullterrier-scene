"""C1: the first picture.

A photo with one image look on it, a vector headline over it, rendered whole at two sizes. The photo is generated here
(no third-party picture); the look is the dict `bullterrier_image.Recipe.to_dict()` writes, and the headline is the JSON
document `bullterrier_vector.Drawing.to_json()` writes, built here from rectangles so that the example needs neither
package. Proves: a picture layer with a look, a drawing layer, normal blend, the same scene at any size.
"""

import numpy as np

import bullterrier_scene as bts

W, H = 1280, 720

# `Recipe([fx.grade(exposure=-0.4, saturation=0.7, temperature=-0.6), fx.vignette(), fx.grain(amount=0.03)]).to_dict()`
NIGHT = {
    "steps": [
        {
            "effect": "grade",
            "params": {"exposure": -0.4, "saturation": 0.7, "temperature": -0.6},
            "opacity": 1.0,
            "blend_mode": "normal",
        },
        {"effect": "vignette", "opacity": 1.0, "blend_mode": "normal"},
        {"effect": "grain", "params": {"amount": 0.03}, "opacity": 1.0, "blend_mode": "normal"},
    ]
}

# the headline in blocks of a 5 by 7 grid, one letter after another
LETTERS = {
    "S": [" ####", "#    ", "#    ", " ### ", "    #", "    #", "#### "],
    "C": [" ####", "#    ", "#    ", "#    ", "#    ", "#    ", " ####"],
    "E": ["#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#####"],
    "N": ["#   #", "##  #", "# # #", "#  ##", "#   #", "#   #", "#   #"],
}
BLOCK = 24  # design pixels of one block
HEADLINE = "SCENE"
INK = "#f4efe6"


def headline(text: str = HEADLINE) -> str:
    """The JSON document of a drawing, `W` by `H`, with `text` in blocks near the bottom left, one path."""
    left, top = 96, H - 96 - 7 * BLOCK
    shapes = []
    for number, letter in enumerate(text):
        for row, line in enumerate(LETTERS[letter]):
            for column, mark in enumerate(line):
                if mark == "#":
                    x = left + (number * 6 + column) * BLOCK
                    y = top + row * BLOCK
                    shapes.append(f"M{x} {y} H{x + BLOCK} V{y + BLOCK} H{x} Z")
    path = " ".join(shapes)
    return (
        f'{{"root":[{{"stack":{{"items":[[{{"path":"{path}"}}]],"size":[{W},{H}]}}}},'
        f'{{"fill":{{"color":"{INK}"}}}}],"version":1}}'
    )


def synthetic_photo() -> np.ndarray:
    """A dusk landscape, `H` by `W` and opaque: a sky from violet to amber, a sun, two hills. Made from arithmetic."""
    y, x = np.mgrid[0:H, 0:W].astype(np.float32)
    height = y / H
    sky = np.stack([0.20 + 0.75 * height, 0.12 + 0.45 * height, 0.38 - 0.15 * height], axis=-1)
    sun = np.clip(1.0 - np.hypot(x - 0.68 * W, y - 0.55 * H) / (0.07 * W), 0.0, 1.0)[..., None]
    picture = sky * (1 - sun) + np.array([1.0, 0.86, 0.55], np.float32) * sun
    far = 0.62 * H + 40 * np.sin(x / 190.0)
    near = 0.78 * H + 28 * np.sin(x / 95.0 + 1.3)
    picture = np.where((y > far)[..., None], np.array([0.20, 0.17, 0.30], np.float32), picture)
    picture = np.where((y > near)[..., None], np.array([0.07, 0.07, 0.12], np.float32), picture)
    return np.clip(picture, 0.0, 1.0).astype(np.float32)


def build(photo: np.ndarray) -> bts.Scene:
    """The scene: `photo` (`H` by `W`) graded by `NIGHT`, and the headline over it."""
    return bts.Scene(W, H).add(
        bts.Layer.picture(photo).look(NIGHT).named("photo"),
        bts.Layer.drawing(headline()).named("headline"),
    )


if __name__ == "__main__":
    from bullterrier_image import imageio

    imageio.save(build(synthetic_photo()).render(W), "first_picture.png")

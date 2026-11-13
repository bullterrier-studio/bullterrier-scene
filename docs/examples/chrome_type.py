"""C5: chrome type.

A vector headline on a drawing layer, lit as polished metal by two looks of image: `liquid_metal` reads the
letters' brightness as a relief and mirrors a sky in it, and `relight` adds a raking light on the same relief.
Every `px` value is in design pixels, so the type looks the same at `render(600)` and `render(1800)`; only the
detail differs. Proves: a look on a drawing layer, a look's space (the fitted drawing), and what a look cannot do.

A look cannot change alpha: the bevel and the chrome show inside the glyphs, and light the look puts outside them
is cut. An emboss shadow outside the letters is a layer of its own, a black fill masked by the type itself,
offset. The fill of the type must be light: a look runs on the layer's colour over black, and the relief is its
brightness.
"""

import bullterrier_vector as btvec
from bullterrier_image import Recipe, fx, imageio

import bullterrier_scene as bts

W, H = 1200, 600

chrome = Recipe(
    [
        fx.liquid_metal(smoothing=3.0, relief=35.0, horizon=0.55, tint="#c9d6ff"),
        fx.relight(field="light", angle=225.0, elevation=40.0, height=8.0, blur=1.5),
    ],
    name="chrome",
)


def build() -> bts.Scene:
    """A dark ground, the type's shadow and the chrome type."""
    headline = btvec.text(
        "CHROME",
        font=btvec.Font.bundled("bricolage_grotesque"),
        size=330,
        axes={"wght": 800},
        anchor=(70, 400),
    ).fill("#ffffff")  # light: the look runs over black
    return bts.Scene(W, H).add(
        bts.Layer.fill("#15151f").named("ground"),
        # the emboss shadow: the type's own outline (its look and alpha), moved down and right, softened
        bts.Layer.fill("#000000")
        .named("shadow")
        .mask(bts.Layer.of("type").place(dx=10, dy=14), feather=8)
        .opacity(0.55),
        bts.Layer.drawing(btvec.Drawing(headline, size=(W, H))).named("type").look(chrome),
    )


if __name__ == "__main__":
    scene = build()
    for size in (600, 1800):  # the same picture at two sizes
        imageio.save(scene.render(size), f"chrome-{size}.png")

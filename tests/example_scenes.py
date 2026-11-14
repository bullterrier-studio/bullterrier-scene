"""The example scenes of `docs/examples`, built from synthetic inputs, for the tests that run over them all: the
programs themselves, or what each is made of (a look, a mask by a drawing and by a map, a reference, a group)."""

import importlib.util
import json
from collections.abc import Callable
from pathlib import Path
from types import ModuleType

import numpy as np

from bullterrier_scene import Layer, Scene

ROOT = Path(__file__).resolve().parents[1]


def load(name: str) -> ModuleType:
    spec = importlib.util.spec_from_file_location(name, ROOT / "docs" / "examples" / f"{name}.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def photo(height: int, width: int) -> np.ndarray:
    """A float32 picture whose every pixel differs."""
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    return np.stack([x / width, y / height, (x + y) % 7 / 7], axis=-1).astype(np.float32)


def matte(height: int, width: int) -> np.ndarray:
    """A float32 map: a disc."""
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    return np.clip(1.5 - np.hypot(x - width / 2, y - height / 2) / (0.25 * width), 0, 1).astype(np.float32)


# a diamond drawn on by the Param `ring_on`, and a bar that shows by the Param `landed`: what mill's logo reads
LOGO = json.dumps(
    {
        "version": 1,
        "params": [
            {"name": "ring_on", "default": 0, "min": 0, "max": 1, "unit": "fraction", "meaning": "how much is drawn"},
            {"name": "landed", "default": 0, "min": 0, "max": 1, "unit": "fraction", "meaning": "the bar shows"},
        ],
        "root": [
            {
                "stack": {
                    "items": [
                        [
                            {"path": "M10 50 L50 10 L90 50 L50 90 Z"},
                            {"slice": {"start": 0, "end": {"param": "ring_on"}}},
                            {"stroke": {"color": "#ff3fd0", "width": 3}},
                        ],
                        [
                            {"path": "M30 40 L70 40 L70 60 L30 60 Z"},
                            {"fill": {"color": "#8ffcff"}},
                            {"opacity": {"alpha": {"param": "landed"}}},
                        ],
                    ],
                    "size": [100, 100],
                }
            }
        ],
    }
)


def cover_template() -> Scene:
    module = load("cover_template")
    return module.build().with_values(module.episode("Pilot", photo(36, 64), "#2b0a3d"))


def frame_sequence() -> Scene:
    module = load("frame_sequence")
    return module.build().with_values(module.frame_values(9))


def mill() -> Scene:
    module = load("mill")
    scene = module.build(photo(160, 90), matte(160, 90), matte(160, 90) * 0.5, LOGO)
    return scene.with_values(module.frame_values(0.7, 0.4, 0.1, 1.0, 1.0, [1, 0.6, 0.2, 0]))


def poster_style() -> Scene:
    """What poster_two_ink is made of: a look, a mask by a drawing and by a map, an inverted reference mask, a blend."""
    w, h = 90, 120
    window = {
        "version": 1,
        "root": [
            {"stack": {"items": [[{"path": "M10 10 L80 5 L85 100 L5 110 Z"}]], "size": [90, 120]}},
            {"fill": {"color": "#ffffff"}},
        ],
    }
    return Scene(w, h).add(
        Layer.fill("#f2ece0").named("paper"),
        Layer.picture(photo(h, w), maps={"subject": matte(h, w)})
        .named("photo")
        .fit((0, 0, w, h), "cover")
        .look({"steps": [{"effect": "grade", "params": {"contrast": 1.4}}]})
        .mask(Layer.drawing(window)),
        Layer.text("MORE", font="bricolage_grotesque", size=40, color="#f2ece0", axes={"wght": 800})
        .named("headline")
        .place(dx=10, dy=60)
        .mask(Layer.of("subject"), invert=True),
        Layer.of("photo").named("subject").mask("subject"),
        Layer.fill("#808080")
        .named("grain")
        .look({"steps": [{"effect": "grain", "params": {"amount": 0.07}}]})
        .blend("overlay"),
    )


def composed_world_style() -> Scene:
    """What composed_world is made of: a cut-out, its shadow and reflection by reference, a group as the sun, a glow."""
    w, h = 120, 80
    sun = Scene(40, 40).add(
        Layer.fill("#ffcc00").named("disc"), Layer.fill("#ff3300").named("stripe").fit((0, 18, 40, 6), "stretch")
    )
    return Scene(w, h).add(
        Layer.fill("#4a3772").named("floor"),
        Layer.scene(sun).named("sun").place(dx=70, dy=4),
        Layer.of("sun")
        .named("sun_glow")
        .look({"steps": [{"effect": "bloom", "params": {"radius": 6.0, "amount": 1.0}}]})
        .as_light()
        .blend("screen"),
        Layer.fill("#000000")
        .named("shadow")
        .mask(Layer.of("dog").place(aspect=0.2, anchor=(0.5, 1.0)), feather=2)
        .opacity(0.7),
        Layer.of("dog").named("reflection").place(flip=(False, True), anchor=(0.5, 1.0)).opacity(0.4),
        Layer.picture(photo(40, 30), maps={"subject": matte(40, 30)}).named("dog").mask("subject").place(dx=20, dy=30),
    )


SCENES: dict[str, Callable[[], Scene]] = {
    "cover_template": cover_template,
    "frame_sequence": frame_sequence,
    "mill": mill,
    "poster_style": poster_style,
    "composed_world_style": composed_world_style,
}


def chrome_style() -> Scene:
    """What chrome_type is made of: a drawing with two looks, its emboss shadow a fill masked by the drawing."""
    chrome = {
        "steps": [
            {"effect": "grade", "params": {"contrast": 1.2}},
            {"effect": "bloom", "params": {"radius": 4.0, "amount": 0.5}},
        ]
    }
    return Scene(100, 100).add(
        Layer.fill("#202030").named("ground"),
        Layer.fill("#000000").named("shadow").mask(Layer.drawing(LOGO).place(dx=2, dy=3), feather=1).opacity(0.55),
        Layer.drawing(LOGO).named("type").look(chrome),
    )


def _built(name: str, *args: object) -> Scene:
    module = load(name)
    scene = module.build(*args)
    assert isinstance(scene, Scene)
    return scene


# the other programs of docs/examples that make one scene (a program that needs a plugin layer, C7, is not here)
MORE_SCENES: dict[str, Callable[[], Scene]] = {
    "first_picture": lambda: load("c1_first_picture").first_picture(),
    "pile_of_prints": lambda: load("c2_prints").pile_of_prints(),
    "delivery_crop": lambda: load("c2_prints").delivery_crop(),
    "receding_floor": lambda: load("c2b_perspective").receding_floor(),
    "clipped_type": lambda: load("c3_clip").clipped_type(),
    "blend_sheet": lambda: load("c4a_blends").blend_sheet(),
    "evening": lambda: _built("c4b_references"),
    "groups": lambda: _built("c4c_groups"),
    "looks_on_everything": lambda: _built("c5a_looks_on_everything"),
    "six_covers": lambda: _built("c6a_covers"),
    "badge": lambda: _built("badge_scene_in_scene"),
    "chrome_style": chrome_style,
}

ALL_SCENES: dict[str, Callable[[], Scene]] = {**SCENES, **MORE_SCENES}

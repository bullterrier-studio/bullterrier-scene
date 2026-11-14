"""The entry-point group `bullterrier_scene.layers`, in a subprocess with a real distribution on its path:
`importlib.metadata` reads the installed distributions, the external boundary."""

import os
import subprocess
import sys
import textwrap
from pathlib import Path

PLUGIN_MODULE = """\
import numpy as np
import bullterrier_scene as bts

@bts.layer_kind("scene-test-flat")
def flat(x, y, width, height, scale_x, scale_y, *, tone=0.5):
    '''A flat grey.'''
    return np.full((round(height * scale_y), round(width * scale_x), 4), tone, np.float32)
"""

BROKEN_MODULE = """\
import os
import bullterrier_scene as bts

if os.path.exists(os.environ["FIXED"]):
    @bts.layer_kind("scene-test-halfway")
    def second_attempt(*arguments):
        '''A different function under the name the first attempt registered.'''
        raise AssertionError
else:
    @bts.layer_kind("scene-test-halfway")
    def first_attempt(*arguments):
        '''Registered, then the import fails.'''
        raise AssertionError
    raise RuntimeError("cannot import")
"""


def install(site: Path, name: str, module_source: str, group: str = "bullterrier_scene.layers") -> None:
    """A distribution `name` in `site` whose entry point `name` is the module `name_mod` with `module_source`."""
    info = site / f"{name}-1.0.dist-info"
    info.mkdir(parents=True)
    (info / "METADATA").write_text(f"Metadata-Version: 2.1\nName: {name}\nVersion: 1.0\n", encoding="utf-8")
    (info / "entry_points.txt").write_text(f"[{group}]\n{name} = {name}_mod\n", encoding="utf-8")
    (site / f"{name}_mod.py").write_text(module_source, encoding="utf-8")


def run(site: Path, script: str, **environment: str) -> None:
    code = f"import sys\nsys.path.insert(0, {str(site)!r})\n" + textwrap.dedent(script)
    result = subprocess.run(
        [sys.executable, "-c", code],
        capture_output=True,
        encoding="utf-8",
        check=False,
        env={**os.environ, **environment},
    )
    assert result.returncode == 0, result.stdout + result.stderr


def test_a_plugin_of_the_group_is_loaded_on_the_first_query_and_not_at_import(tmp_path):
    install(tmp_path, "pumpkin", PLUGIN_MODULE)
    run(
        tmp_path,
        """
        import bullterrier_scene as bts
        assert "pumpkin_mod" not in sys.modules, "imported at import time"
        assert "scene-test-flat" in bts.layer_kinds()
        assert "pumpkin_mod" in sys.modules
        picture = bts.Scene(4, 4).add(bts.Layer.make("scene-test-flat", tone=0.25)).render(4)
        assert picture[0, 0].tolist() == [0.25] * 4, picture[0, 0]
        """,
    )


def test_layer_make_and_from_json_load_the_group_too(tmp_path):
    install(tmp_path, "pumpkin", PLUGIN_MODULE)
    run(
        tmp_path,
        """
        import bullterrier_scene as bts
        layer = '{"make":{"kind":"scene-test-flat","options":{}}}'
        text = '{"version":1,"canvas":{"width":4,"height":4},"layers":[' + layer + ']}'
        assert "pumpkin_mod" not in sys.modules
        bts.Scene.from_json(text)
        assert "pumpkin_mod" in sys.modules
        """,
    )
    run(
        tmp_path,
        """
        import bullterrier_scene as bts
        bts.Layer.make("scene-test-flat")
        assert "pumpkin_mod" in sys.modules
        """,
    )


def test_a_plugin_in_another_group_is_not_loaded(tmp_path):
    install(tmp_path, "pumpkin", PLUGIN_MODULE, group="something.else")
    run(
        tmp_path,
        """
        import bullterrier_scene as bts
        assert bts.layer_kinds() == [] and "pumpkin_mod" not in sys.modules
        """,
    )


def test_a_plugin_that_fails_to_import_is_named_and_leaves_nothing_registered_until_it_is_fixed(tmp_path):
    install(tmp_path, "agood", PLUGIN_MODULE)
    install(tmp_path, "broken", BROKEN_MODULE)
    fixed = tmp_path / "fixed"
    run(
        tmp_path,
        f"""
        import os
        import bullterrier_scene as bts
        try:
            bts.layer_kinds()
        except bts.BullterrierSceneError as error:
            assert "broken" in str(error) and "cannot import" in str(error), str(error)
            assert isinstance(error.__cause__, RuntimeError)
        else:
            raise AssertionError("no error")
        open({str(fixed)!r}, "w").close()   # the plugin is repaired; the next query loads it afresh
        names = bts.layer_kinds()   # a leftover of the first attempt would clash with the second function
        assert names == ["scene-test-flat", "scene-test-halfway"], names
        """,
        FIXED=str(fixed),
    )

"""`uv run poe bench-gpu`, second part: a frame of the mill piece (`docs/examples/mill.py`, 720 by 1280, drawn at 1,280
on the long side) on the CPU and on the GPU. The first frame is cold; the next ones set the moving values
(`frame_values`) as an animation does, so what a frame does not change comes from the scene's cache (CPU) or the
device's (GPU). The GPU time includes the read-back into a numpy array (what `render(gpu=)` gives). Medians, in ms."""

import statistics
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from tests.example_scenes import LOGO, load, matte, photo

from bullterrier_scene import Gpu

FRAMES = 12
SIZE = 1280


def main() -> None:
    """Print the first frame's time and the median of the rest, on each backend."""
    mill = load("mill")
    base = mill.build(photo(1280, 720), matte(1280, 720), matte(1280, 720) * 0.5, LOGO)
    gpu = Gpu()
    print(f"adapter: {gpu.adapter}")
    frames = []
    for n in range(FRAMES):
        t = n / (FRAMES - 1)
        values = mill.frame_values(
            t, abs(1 - 2 * t), t**2, min(1.0, 2 * t), max(0.0, min(1.0, 3 * t - 1.0)), [t, t * 0.6, t * 0.3, 0]
        )
        frames.append(base.with_values(values))
    for name, render in (("CPU", lambda scene: scene.render(SIZE)), ("GPU", lambda scene: scene.render(SIZE, gpu=gpu))):
        times = []
        for scene in frames:
            start = time.perf_counter()
            render(scene)
            times.append((time.perf_counter() - start) * 1000)
        print(f"mill frame, {name}: first (cold) {times[0]:.0f} ms, then median {statistics.median(times[1:]):.0f} ms")


main()

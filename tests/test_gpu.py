"""`Gpu` and `render(size, gpu=)`: what Python code can break on the way to the core's GPU compositor (the kernels'
parity is the core's own test). `BT_GPU=none` says the machine has no GPU: the tests that need one say so and skip;
without it a missing GPU fails them."""

import os
import platform
import sys

import numpy as np
import pytest

import bullterrier_scene as bts
from bullterrier_scene import Gpu, GpuError, Layer, Scene

NO_GPU = os.environ.get("BT_GPU") == "none"
needs_gpu = pytest.mark.skipif(NO_GPU, reason="BT_GPU=none: this machine has no GPU")


@pytest.mark.skipif(
    sys.platform == "darwin" or os.environ.get("BT_GPU_EXPERIMENTAL") == "1",
    reason="the GPU backend runs on macOS, and elsewhere only with BT_GPU_EXPERIMENTAL=1",
)
def test_a_gpu_is_refused_by_name_on_a_platform_the_backend_does_not_support():
    # not behind BT_GPU=none: the refusal needs no GPU, only the wheel built with the compositor
    with pytest.raises(GpuError, match="macOS and the browser") as refused:
        Gpu()
    assert platform.system().lower() in str(refused.value)
    assert "BT_GPU_EXPERIMENTAL=1" in str(refused.value)


def poster() -> Scene:
    ground = Layer.fill("#204080")
    noise = np.random.default_rng(3).random((20, 30, 4), dtype=np.float32)
    return Scene(60, 40).add(ground, Layer.picture(noise).fit((0, 0, 60, 40), "cover").opacity(0.7))


@needs_gpu
def test_a_render_on_the_gpu_is_the_cpu_render_within_the_bound():
    gpu = Gpu()
    on_cpu = poster().render(60)
    on_gpu = poster().render(60, gpu=gpu)
    assert on_gpu.dtype == np.float32
    assert on_gpu.shape == on_cpu.shape
    assert float(np.abs(on_gpu - on_cpu).max()) <= 1e-4


@needs_gpu
def test_the_context_counts_what_crosses_and_names_its_adapter():
    gpu = Gpu()
    assert gpu.adapter
    poster().render(60, gpu=gpu)
    uploads, downloads, size = gpu.transfers()
    assert (uploads, downloads) == (2, 1)
    assert size > 0
    gpu.reset_transfers()
    assert gpu.transfers() == (0, 0, 0)


@needs_gpu
def test_the_device_cache_budget_is_settable():
    gpu = Gpu()
    gpu.cache_budget = 123_456
    assert gpu.cache_budget == 123_456


@needs_gpu
def test_a_size_the_scene_refuses_is_the_scenes_own_error_not_the_machines():
    with pytest.raises(bts.InvalidError, match="size"):
        poster().render(0, gpu=Gpu())

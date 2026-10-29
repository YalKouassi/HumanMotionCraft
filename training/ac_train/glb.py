"""Read a glTF 2 binary (.glb): its JSON and binary chunk, and accessors as numpy arrays."""

from __future__ import annotations

import json
import pathlib
import struct

import numpy as np

_COMPONENTS = {5121: np.uint8, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
_WIDTH = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


def read_glb(path) -> tuple[dict, bytearray]:
    data = pathlib.Path(path).read_bytes()
    magic, version, _ = struct.unpack_from("<III", data, 0)
    if magic != 0x46546C67 or version != 2:
        raise ValueError(f"{path}: not a glTF 2 binary")
    off, gltf, binary = 12, None, bytearray()
    while off < len(data):
        length, kind = struct.unpack_from("<II", data, off)
        chunk = data[off + 8 : off + 8 + length]
        if kind == 0x4E4F534A:
            gltf = json.loads(chunk.decode("utf-8"))
        elif kind == 0x004E4942:
            binary = bytearray(chunk)
        off += 8 + length
    if gltf is None:
        raise ValueError(f"{path}: no JSON chunk")
    return gltf, binary


def read_accessor(gltf: dict, binary: bytearray, index: int) -> np.ndarray:
    acc = gltf["accessors"][index]
    view = gltf["bufferViews"][acc["bufferView"]]
    dtype = np.dtype(_COMPONENTS[acc["componentType"]])
    width = _WIDTH[acc["type"]]
    start = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
    stride = view.get("byteStride", 0)
    if stride and stride != dtype.itemsize * width:
        raise ValueError("interleaved accessors are not supported")
    a = np.frombuffer(binary, dtype=dtype, count=acc["count"] * width, offset=start)
    return a.reshape(acc["count"], width).copy()

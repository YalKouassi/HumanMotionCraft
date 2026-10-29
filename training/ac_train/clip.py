"""Keyframed clips on a named skeleton.

A key holds one rotation per bone (x, y, z, w, relative to the parent with every bone at rest along
the world axes) and a root offset, the same space as the rig's pose library, so a sampled clip goes
straight into the pose library's local-frame conversion. Keys sit on integer frames; :meth:`Clip.sample`
interpolates between them.

Interpolation per key (the segment that starts at that key):
- ``"step"``: hold the key until the next one.
- ``"linear"``: slerp rotations, lerp the root.
- ``"smooth"`` (default): squad rotations, cubic root, with clamped (monotone) tangents from the
  neighbouring keys: flat where the keys hold or turn back, so a curve never swings past its keys.

Partial keys: a key with ``only`` (bone indices) sets just those bones; every bone interpolates
between the keys that set it, so keying the arms on a frame leaves the legs moving between their own
keys. The root offset belongs to the root bone (index 0). A key still stores every bone (the pose
seen when it was set); the bones outside ``only`` are not used.

Layers: the keys above are the base layer. More layers stack on top, bottom to top (as in
game engines): a layer is its own keys or a link to another animation of the library, drives
some bones (``only``), and either replaces them (``override``) or adds a turn to them
(``additive``: its keys hold the turn), blended by ``weight``. A link plays the other animation with
a time offset / speed / loop and starts where the character stands. ``Clip.keys`` is the keys of
the layer being edited (``edit``), so every key tool works on that layer.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field

import numpy as np

from . import quat

FORMAT = "cf-animate.clip"
VERSION = 1
INTERPOLATIONS = ("step", "linear", "smooth", "ease_in", "ease_out", "ease")
# easings: the smooth curve's path, its timing reshaped (slow start / slow end / both)
EASE = {
    "ease_in": lambda t: t * t,
    "ease_out": lambda t: 1 - (1 - t) * (1 - t),
    "ease": lambda t: t * t * (3 - 2 * t),
}



def _slope(d_in, d_out, h_in: float, h_out: float) -> np.ndarray:
    """A smooth curve's slope (per frame) at a key, from the change ``d_in`` over the ``h_in`` frames
    before it and ``d_out`` over the ``h_out`` frames after. Flat where the keys hold or turn back,
    and at most three times the gentler side's slope, so the curve never swings past a key's
    neighbours (monotone cubic, Fritsch-Carlson): a hold before a long move stays put. An end key
    (``h_in`` or ``h_out`` 0) takes half its one segment's slope."""
    d_in, d_out = np.asarray(d_in, dtype=np.float64), np.asarray(d_out, dtype=np.float64)
    if h_in <= 0:
        return d_out / h_out / 2 if h_out > 0 else np.zeros_like(d_out)
    if h_out <= 0:
        return d_in / h_in / 2
    s_in, s_out = d_in / h_in, d_out / h_out
    limit = 3 * np.minimum(np.abs(s_in), np.abs(s_out))
    m = np.clip((d_in + d_out) / (h_in + h_out), -limit, limit)
    return np.where(s_in * s_out > 0, m, 0.0)

@dataclass
class Key:
    frame: int
    rotations: np.ndarray  # (bones, 4)
    root: np.ndarray  # (3,)
    interp: str = "smooth"
    source: str = "manual"  # "manual" (posed by hand) or "ai" (made by the in-betweener)
    model: str = ""  # the in-betweener that made an "ai" key
    only: tuple[int, ...] | None = None  # the bones this key sets (None: all of them)

    def __post_init__(self):
        self.frame = int(self.frame)
        if self.source not in ("manual", "ai"):
            raise ValueError(f"unknown key source {self.source!r}")
        r = np.asarray(self.rotations, dtype=np.float64)
        # normalise only what is off: re-normalising unit quaternions moves their last bit, and then
        # load -> save would not give back the same bytes
        n = np.linalg.norm(r, axis=-1, keepdims=True)
        self.rotations = np.where(np.abs(n - 1) > 1e-12, r / n, r)
        self.root = np.asarray(self.root, dtype=np.float64).reshape(3)
        if self.only is not None:
            self.only = tuple(sorted({int(i) for i in self.only}))
            if not self.only:
                raise ValueError("a partial key needs at least one bone")
            if len(self.only) == len(self.rotations):
                self.only = None

    def sets(self, bone: int) -> bool:
        return self.only is None or bone in self.only
        if self.interp not in INTERPOLATIONS:
            raise ValueError(f"unknown interpolation {self.interp!r}; expected one of {INTERPOLATIONS}")


LAYER_SOURCES = ("keys", "link", "travel")
LAYER_MODES = ("override", "additive")


@dataclass
class Layer:
    name: str = "Layer"
    source: str = "keys"  # "keys": its own keys; "link": another animation; "travel": a planned walk (its keys)
    keys: list[Key] = field(default_factory=list)
    link: str = ""  # the linked animation's id ("<group>/<name>")
    only: tuple[int, ...] | None = None  # the bones it drives (None: all)
    parts: list[str] = field(default_factory=list)  # the part names it was made from (for the UI)
    mode: str = "override"
    weight: float = 1.0
    offset: float = 0.0  # a link starts on this frame
    speed: float = 1.0
    loop: bool = True  # a link repeats (and keeps travelling) after its end
    muted: bool = False
    start: float | None = None  # a layer with a range drives frames start..end only,
    end: float | None = None  # fading in and out over ``fade`` frames at its ends
    fade: int = 0
    gait: str = "auto"  # a travel layer's gait
    waypoints: list = field(default_factory=list)  # a travel layer's path: [x, y] (m) on the floor
    moves: dict = field(default_factory=dict)  # footsteps moved by hand: {"L0": [dx, dy] (m)}
    steps: list = field(default_factory=list)  # the planned footsteps [side, first, last, x, y, z]

    def ramp(self, frames: np.ndarray) -> np.ndarray:
        """The layer's weight on each frame (its range and fades)."""
        w = np.full(len(frames), self.weight)
        if self.start is None or self.end is None:
            return w
        f = np.asarray(frames, dtype=np.float64)
        inside = (f >= self.start) & (f <= self.end)
        if self.fade > 0:
            edge = np.minimum(f - self.start, self.end - f) / self.fade
            return w * np.where(inside, np.clip(edge, 0, 1), 0.0)
        return w * inside

    def __post_init__(self):
        if self.source not in LAYER_SOURCES:
            raise ValueError(f"unknown layer source {self.source!r}")
        if self.mode not in LAYER_MODES:
            raise ValueError(f"unknown layer mode {self.mode!r}")
        if self.only is not None:
            self.only = tuple(sorted({int(i) for i in self.only}))
        self.weight = float(min(max(self.weight, 0.0), 1.0))
        self.offset, self.speed = float(self.offset), float(self.speed)
        if self.speed <= 0:
            raise ValueError("a layer's speed must be above 0")


class Clip:
    def __init__(self, bones, fps: float = 30.0, loop: bool = False, keys=None, name: str = "", group: str = "",
                 origin: str = "", layers=None, dataset: str = ""):
        self.bones = list(bones)
        self.fps = float(fps)
        self.loop = bool(loop)
        self.name = name
        self.group = group  # your grouping in the Animations list
        self.origin = origin  # where a built-in came from ("osb", "mocap"); "" for yours
        self.dataset = dataset  # for a mocap built-in: the dataset it was cut from (its licence)
        self.layers: list[Layer] = [Layer("Base", keys=list(keys or []))] + list(layers or [])
        self.edit = 0  # the layer the key tools work on
        self.physics = True  # play and export through the physics pass (ac_core::physics)
        # boxes in the scene the body stands on, puts a hand on or jumps over:
        # {"name", "min": [x, y], "max": [x, y] (m, rig world), "top": height above the floor (m)}
        self.surfaces: list[dict] = []
        # id -> Clip for linked layers (a plain function: deepcopy keeps it as it is)
        self.resolve = None

    def __repr__(self):
        return f"Clip({self.name!r}, {len(self.bones)} bones, {len(self.layers)} layers)"

    @property
    def keys(self) -> list[Key]:
        return self.layers[self.edit].keys

    @keys.setter
    def keys(self, value) -> None:
        self.layers[self.edit].keys = list(value)

    @property
    def base(self) -> Layer:
        return self.layers[0]

    # ---- editing -------------------------------------------------------------------------

    def set_key(self, key: Key) -> None:
        """Insert a key, replacing any key on the same frame."""
        if key.rotations.shape != (len(self.bones), 4):
            raise ValueError(f"key has {key.rotations.shape[0]} bones, clip has {len(self.bones)}")
        self.keys = [k for k in self.keys if k.frame != key.frame] + [key]
        self.keys.sort(key=lambda k: k.frame)

    def remove_key(self, frame: int) -> None:
        before = len(self.keys)
        self.keys = [k for k in self.keys if k.frame != frame]
        if len(self.keys) == before:
            raise KeyError(f"no key on frame {frame}")

    @property
    def length(self) -> int:
        """Frames covered: the last key of any layer + 1 (a looping clip wraps after it)."""
        last = [lay.keys[-1].frame for lay in self.layers if lay.source != "link" and lay.keys]
        for lay in self.layers[1:]:  # a link counts with one play-through
            src = self.resolve(lay.link) if lay.source == "link" and self.resolve and lay.link else None
            if src is not None and src.length:
                last.append(int(np.ceil(lay.offset + (src.length - 1) / lay.speed)))
        return max(last) + 1 if last else 0

    # ---- sampling ------------------------------------------------------------------------

    def sample(self, frames) -> tuple[np.ndarray, np.ndarray]:
        """Rotations (F, bones, 4) and root offsets (F, 3) at (possibly fractional) frames: every
        layer, bottom to top."""
        return self.sample_below(frames, len(self.layers))

    def sample_below(self, frames, top: int) -> tuple[np.ndarray, np.ndarray]:
        """The layers under ``top`` only (``top`` = 1: the base keys alone)."""
        if not self.base.keys:
            raise ValueError("clip has no keys")
        frames = np.atleast_1d(np.asarray(frames, dtype=np.float64))
        rots, roots = self._view(self.base.keys)._sample_keys(frames)
        for i in range(1, min(top, len(self.layers))):
            lay = self.layers[i]
            if lay.muted or lay.weight == 0:
                continue
            got = self._layer_pose(lay, frames, i)
            if got is None:
                continue
            lr, lo = got
            bones = list(range(len(self.bones))) if lay.only is None else list(lay.only)
            w = lay.ramp(frames)
            if not w.any():
                continue
            full = bool((w == 1).all())
            wq = w[:, None]  # slerp broadcasts it over the bones
            if lay.mode == "override":
                rots[:, bones] = lr[:, bones] if full else quat.slerp(rots[:, bones], lr[:, bones], wq)
                if 0 in bones:
                    roots = lo if full else roots + (lo - roots) * w[:, None]
            else:  # additive: the layer's pose is a turn on top of what is below
                d = lr[:, bones] if full else quat.slerp(np.broadcast_to([0.0, 0.0, 0.0, 1.0], lr[:, bones].shape), lr[:, bones], wq)
                rots[:, bones] = quat.normalize(quat.mul(rots[:, bones], d))
                if 0 in bones:
                    roots = roots + lo * w[:, None]
        return rots, roots

    def _view(self, keys) -> "Clip":
        """A clip of these keys alone (same bones, fps, loop)."""
        v = Clip(self.bones, self.fps, self.loop)
        v.base.keys = list(keys)
        return v

    def _layer_pose(self, lay: Layer, frames: np.ndarray, index: int):
        """A layer's own pose (an additive layer: its turn); None when it has nothing to give."""
        if lay.source != "link":
            if not lay.keys:
                return None
            return self._view(lay.keys)._sample_keys(frames)
        src = self.resolve(lay.link) if self.resolve and lay.link else None
        if src is None or not src.base.keys or list(src.bones) != list(self.bones):
            return None  # a missing link drives nothing
        n = src.length
        period = n if src.loop else max(n - 1, 1)
        t = (frames - lay.offset) * lay.speed
        if lay.loop:
            cycles = np.floor(t / period)
            t = t - cycles * period
        else:
            cycles = np.zeros_like(t)
            t = np.clip(t, 0, n - 1)
        r, o = src.sample(t)
        r0, o0 = src.sample([0.0])
        # a repeating link keeps travelling: each cycle adds its own ground distance
        end = src.sample([float(period)])[1][0] if src.loop else src.sample([float(n - 1)])[1][0]
        step = end - o0[0]
        o = o.copy()
        o[:, :2] += cycles[:, None] * step[:2]
        if lay.mode == "additive":  # the turn away from the link's own first frame
            return quat.normalize(quat.mul(quat.conj(r0), r)), o - o0[0]
        # the link starts where the character stands (on the floor) at the layer's first frame
        below = self.sample_below([lay.offset], index)[1][0]
        o[:, :2] += below[:2] - o0[0, :2]
        # and at its height: clips differ in where they put the floor
        o[:, 2] += below[2] - o0[0, 2]
        return r, o

    def _sample_keys(self, frames: np.ndarray):
        """The base keys of this clip alone (partial keys per bone channel)."""
        if any(k.only is not None for k in self.keys):
            return self._sample_channels(frames)
        return self._sample_plain(frames)

    def channels(self) -> dict[tuple[int, ...], list[int]]:
        """Bones grouped by the keys that set them: {key indices: bones}. A bone no key sets
        follows every key (it keeps the pose the keys were set with)."""
        groups: dict[tuple[int, ...], list[int]] = {}
        for b in range(len(self.bones)):
            ks = tuple(i for i, k in enumerate(self.keys) if k.sets(b)) or tuple(range(len(self.keys)))
            groups.setdefault(ks, []).append(b)
        return groups

    def _sample_channels(self, frames: np.ndarray):
        rots = np.empty((len(frames), len(self.bones), 4))
        roots = None
        for ks, bs in self.channels().items():
            sub = Clip(bones=self.bones, fps=self.fps, loop=self.loop)
            sub.keys = [self.keys[i] for i in ks]
            r, o = sub._sample_plain(frames)
            rots[:, bs] = r[:, bs]
            if 0 in bs:
                roots = o
        return rots, roots

    def _sample_plain(self, frames: np.ndarray):
        rots = np.empty((len(frames), len(self.bones), 4))
        roots = np.empty((len(frames), 3))
        for i, f in enumerate(frames):
            rots[i], roots[i] = self._sample_one(float(f))
        return rots, roots

    def sample_all(self) -> tuple[np.ndarray, np.ndarray]:
        """One sample per integer frame from 0 to :attr:`length` - 1."""
        return self.sample(np.arange(self.length))

    def _neighbours(self, f: float):
        """Keys k0 <= f < k1 plus their outer neighbours, and the segment parameter t."""
        ks = self.keys
        n = len(ks)
        if n == 1:
            return ks[0], ks[0], ks[0], ks[0], 0.0
        if self.loop:
            # the last key wraps to the first, one frame later
            period = ks[-1].frame + 1 - ks[0].frame
            f = ks[0].frame + (f - ks[0].frame) % period
            frames = [k.frame for k in ks]
            i = int(np.searchsorted(frames, f, side="right")) - 1
            k0, k1 = ks[i], ks[(i + 1) % n]
            span = (k1.frame - k0.frame) % period or period
            t = (f - k0.frame) / span
            return ks[(i - 1) % n], k0, k1, ks[(i + 2) % n], t
        if f <= ks[0].frame:
            return ks[0], ks[0], ks[0], ks[0], 0.0
        if f >= ks[-1].frame:
            return ks[-1], ks[-1], ks[-1], ks[-1], 0.0
        frames = [k.frame for k in ks]
        i = int(np.searchsorted(frames, f, side="right")) - 1
        k0, k1 = ks[i], ks[i + 1]
        t = (f - k0.frame) / (k1.frame - k0.frame)
        return ks[max(i - 1, 0)], k0, k1, ks[min(i + 2, n - 1)], t

    def _sample_one(self, f: float):
        kp, k0, k1, kn, t = self._neighbours(f)
        if k0.interp in EASE:
            t = EASE[k0.interp](t)
        if t == 0.0 or k0.interp == "step":
            return k0.rotations.copy(), k0.root.copy()
        if k0.interp == "linear":
            return quat.slerp(k0.rotations, k1.rotations, t), k0.root + (k1.root - k0.root) * t
        # frames from each key to the next (0: no key on that side)
        period = self.keys[-1].frame + 1 - self.keys[0].frame

        def span(a, b) -> float:
            d = float(b.frame - a.frame)
            return d % period if self.loop else d

        h_in, h, h_out = span(kp, k0), span(k0, k1), span(k1, kn)
        h = h if h > 0 else float(period)
        q0 = k0.rotations
        q1 = quat.align(k1.rotations, q0)
        qp = quat.align(kp.rotations, q0)
        qn = quat.align(kn.rotations, q1)
        # the turns into, across and out of the segment, as rotation vectors (each seen from the
        # key it starts at), and squad's inner controls for the clamped tangents at both ends
        into = quat.log(quat.mul(quat.conj(qp), q0))
        across = quat.log(quat.mul(quat.conj(q0), q1))
        back = quat.log(quat.mul(quat.conj(q1), q0))
        out = quat.log(quat.mul(quat.conj(q1), qn))
        t0 = _slope(into, across, h_in, h) * h
        t1 = _slope(-back, out, h, h_out) * h
        s0 = quat.normalize(quat.mul(q0, quat.exp(0.5 * (t0 - across))))
        s1 = quat.normalize(quat.mul(q1, quat.exp(-0.5 * (t1 + back))))
        rot = quat.squad_arc(q0, q1, s0, s1, t)
        # cubic Hermite on the root, with the same clamped tangents
        m0 = _slope(k0.root - kp.root, k1.root - k0.root, h_in, h) * h
        m1 = _slope(k1.root - k0.root, kn.root - k1.root, h, h_out) * h
        t2, t3 = t * t, t * t * t
        root = (
            (2 * t3 - 3 * t2 + 1) * k0.root
            + (t3 - 2 * t2 + t) * m0
            + (-2 * t3 + 3 * t2) * k1.root
            + (t3 - t2) * m1
        )
        return rot, root

    # ---- serialization -------------------------------------------------------------------

    @staticmethod
    def _keys_dict(bones, keys) -> list[dict]:
        return [
            {
                "frame": k.frame,
                "interp": k.interp,
                **({"source": "ai", "model": k.model} if k.source == "ai" else {}),
                **({"only": [bones[i] for i in k.only]} if k.only is not None else {}),
                "root": [float(x) for x in k.root],
                "rotations": [[float(x) for x in q] for q in k.rotations],
            }
            for k in keys
        ]

    def _layer_dict(self, lay: Layer) -> dict:
        return {
            "name": lay.name,
            "source": lay.source,
            **({"link": lay.link} if lay.source == "link" else {"keys": self._keys_dict(self.bones, lay.keys)}),
            **({"only": [self.bones[i] for i in lay.only]} if lay.only is not None else {}),
            **({"parts": list(lay.parts)} if lay.parts else {}),
            "mode": lay.mode,
            "weight": lay.weight,
            "offset": lay.offset,
            "speed": lay.speed,
            "loop": lay.loop,
            "muted": lay.muted,
            **({"start": lay.start, "end": lay.end, "fade": lay.fade} if lay.start is not None else {}),
            **({"gait": lay.gait} if lay.source == "travel" else {}),
            **({"waypoints": [[float(x) for x in w] for w in lay.waypoints]} if lay.waypoints else {}),
            **({"moves": {k: [float(x) for x in v] for k, v in sorted(lay.moves.items())}} if lay.moves else {}),
            **({"steps": [[st[0], int(st[1]), int(st[2])] + [float(x) for x in st[3:]] for st in lay.steps]} if lay.steps else {}),
        }

    def to_dict(self) -> dict:
        return {
            "format": FORMAT,
            "version": VERSION,
            "name": self.name,
            **({"group": self.group} if self.group else {}),
            **({"origin": self.origin} if self.origin else {}),
            **({"dataset": self.dataset} if self.dataset else {}),
            **({"physics": False} if not self.physics else {}),
            **({"surfaces": [{"name": s["name"], "min": [float(x) for x in s["min"]], "max": [float(x) for x in s["max"]],
                              "top": float(s["top"])} for s in self.surfaces]} if self.surfaces else {}),
            "fps": self.fps,
            "loop": self.loop,
            "bones": list(self.bones),
            "keys": self._keys_dict(self.bones, self.base.keys),
            # a clip with no layers above the base writes none: such files stay as they always were
            **({"layers": [self._layer_dict(lay) for lay in self.layers[1:]]} if len(self.layers) > 1 else {}),
        }

    def dumps(self) -> str:
        # repr of a float64 round-trips exactly, so save -> load -> save is byte-identical
        return json.dumps(self.to_dict(), indent=1, sort_keys=True)

    @classmethod
    def from_dict(cls, d: dict) -> "Clip":
        if d.get("format") != FORMAT:
            raise ValueError(f"not a {FORMAT} file")
        if d.get("version") != VERSION:
            raise ValueError(f"unsupported {FORMAT} version {d.get('version')}")
        clip = cls(bones=list(d["bones"]), fps=float(d["fps"]), loop=bool(d["loop"]), name=d.get("name", ""),
                   group=d.get("group", ""), origin=d.get("origin", ""), dataset=d.get("dataset", ""))
        clip.physics = bool(d.get("physics", True))
        clip.surfaces = [dict(s) for s in d.get("surfaces", [])]
        index = {b: i for i, b in enumerate(clip.bones)}

        def keys_of(items):
            out = Clip(clip.bones)
            for k in items:
                only = [index[b] for b in k["only"]] if "only" in k else None
                out.set_key(Key(k["frame"], np.array(k["rotations"]), np.array(k["root"]), k["interp"],
                                k.get("source", "manual"), k.get("model", ""), only))
            return out.keys

        clip.base.keys = keys_of(d["keys"])
        for ld in d.get("layers", []):
            clip.layers.append(Layer(
                ld["name"], ld["source"], keys_of(ld.get("keys", [])), ld.get("link", ""),
                [index[b] for b in ld["only"]] if "only" in ld else None, list(ld.get("parts", [])),
                ld["mode"], ld["weight"], ld["offset"], ld["speed"], ld["loop"], ld["muted"],
                ld.get("start"), ld.get("end"), ld.get("fade", 0), ld.get("gait", "auto"),
                [list(w) for w in ld.get("waypoints", [])], dict(ld.get("moves", {})), [list(st) for st in ld.get("steps", [])]))
        return clip

    @classmethod
    def loads(cls, text: str) -> "Clip":
        return cls.from_dict(json.loads(text))

    def save(self, path) -> None:
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(self.dumps())

    @classmethod
    def load(cls, path) -> "Clip":
        with open(path, encoding="utf-8") as f:
            return cls.loads(f.read())

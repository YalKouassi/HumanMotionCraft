"""Pure-Python reader and HTTP-Range fetcher for AddBiomechanics ``.b3d`` (``SubjectOnDisk``) files.

``nimblephysics`` does not publish Windows wheels on PyPI, and a ``.b3d`` file is an 8-byte
little-endian ``int64`` header length followed by a Protobuf ``SubjectOnDiskHeader``, then
fixed-size ``SubjectOnDiskSensorFrame`` and ``SubjectOnDiskProcessingPassFrame`` messages for every
frame of every trial. Each processing pass frame already stores ``world_frame_joint_centers``
(field 16: 3D world positions in metres, Y up) and ``ground_contact_force`` (field 8: 3D ground
reaction forces in Newtons on the contact bodies), so we do not need OpenSim forward kinematics at
build time.

To fetch the overground ``With_Arm`` studies from SimTK without downloading the 418 GB archive:

    uv run python -m ac_train.b3d
"""

from __future__ import annotations

import argparse
import pathlib
import struct
import urllib.request
import xml.etree.ElementTree as ET
import zlib
from dataclasses import dataclass

import numpy as np

from . import REPO

ARCHIVE_URL = "http://archive.simtk.org/addbiomechanics/addbiomechanics.zip"
RAW_DIR = REPO / "training" / "data" / "raw" / "addbiomechanics"

#: Overground ``With_Arm`` studies in AddBiomechanics 1.0 (excludes the multi-hour treadmill-only
#: studies ``Carter2023``, ``Moore2015``, ``vanderZee2022`` and ``Hammer2013``).
OVERGROUND_STUDIES: tuple[str, ...] = (
    "Falisse2017_Formatted_With_Arm",
    "Fregly2012_Formatted_With_Arm",
    "Uhlrich2023_Formatted_With_Arm",
    "Tiziana2019_Formatted_With_Arm",
    "Han2023_Formatted_With_Arm",
)


@dataclass
class B3dTrial:
    name: str
    fps: float
    joints: list[str]
    joint_centers: np.ndarray  # (F, J, 3) float64, metres, Y up
    contact_bodies: list[str]
    grf: np.ndarray  # (F, C, 3) float64, Newtons, Y up


def _parse_proto(buf: bytes | memoryview) -> dict[int, list]:
    """Minimal Protobuf wire-format reader (varint, 64-bit float, length-delimited bytes, 32-bit float)."""
    out: dict[int, list] = {}
    i, n = 0, len(buf)
    while i < n:
        tag, shift = 0, 0
        while True:
            byte = buf[i]
            i += 1
            tag |= (byte & 0x7F) << shift
            if not (byte & 0x80):
                break
            shift += 7
        field, wtype = tag >> 3, tag & 7
        if wtype == 0:
            val, shift = 0, 0
            while True:
                byte = buf[i]
                i += 1
                val |= (byte & 0x7F) << shift
                if not (byte & 0x80):
                    break
                shift += 7
        elif wtype == 1:
            val = struct.unpack_from("<d", buf, i)[0]
            i += 8
        elif wtype == 2:
            ln, shift = 0, 0
            while True:
                byte = buf[i]
                i += 1
                ln |= (byte & 0x7F) << shift
                if not (byte & 0x80):
                    break
                shift += 7
            val = bytes(buf[i : i + ln])
            i += ln
        elif wtype == 5:
            val = struct.unpack_from("<f", buf, i)[0]
            i += 4
        else:
            raise ValueError(f"unexpected protobuf wire type {wtype} at byte {i}")
        out.setdefault(field, []).append(val)
    return out


def _joint_names_from_osim(xml_text: str) -> list[str]:
    root = ET.fromstring(xml_text)
    names = [j.attrib.get("name", "") for j in root.findall(".//JointSet/objects/*")]
    if not names:
        names = [j.attrib.get("name", "") for b in root.findall(".//Body") for j in b.findall("./Joint/*")]
    # ``world_frame_joint_centers`` omits patellofemoral constraint joints when 20 centers are stored
    return [n for n in names if n]


def load_bytes(raw: bytes | memoryview) -> list[B3dTrial]:
    """Parse all trials from an uncompressed ``.b3d`` byte buffer."""
    view = memoryview(raw)
    if len(view) < 8:
        raise ValueError("b3d buffer too short")
    header_size = struct.unpack_from("<q", view, 0)[0]
    if header_size <= 0 or 8 + header_size > len(view):
        raise ValueError(f"invalid b3d header size: {header_size}")
    hdr = _parse_proto(view[8 : 8 + header_size])
    sensor_sz = int(hdr[3][0])
    pass_sz = int(hdr[4][0])
    contact_bodies = [b.decode("utf-8", "replace") for b in hdr.get(6, [])]
    osim_joints: list[str] = []
    if 5 in hdr:
        p0 = _parse_proto(hdr[5][0])
        if 2 in p0:
            osim_joints = _joint_names_from_osim(p0[2][0].decode("utf-8", "replace"))

    trials: list[B3dTrial] = []
    off = 8 + header_size
    for tb in hdr.get(9, []):
        th = _parse_proto(tb)
        tname = th.get(1, [b""])[0].decode("utf-8", "replace")
        tlen = int(th.get(3, [0])[0])
        dt = float(th.get(4, [0.0])[0])
        npasses = len(th.get(5, []))
        frame_sz = sensor_sz + npasses * pass_sz
        if tlen <= 0 or dt <= 0.0 or npasses <= 0 or off + tlen * frame_sz > len(view):
            off += max(0, tlen * frame_sz)
            continue

        jcs_list: list[np.ndarray] = []
        grf_list: list[np.ndarray] = []
        n_contacts = max(1, len(contact_bodies))
        for f in range(tlen):
            f_off = off + f * frame_sz
            p_off = f_off + sensor_sz + (npasses - 1) * pass_sz
            pf = _parse_proto(view[p_off : p_off + pass_sz])
            if 16 not in pf:
                break
            jc = np.frombuffer(pf[16][0], dtype="<f8").reshape(-1, 3)
            jcs_list.append(jc)
            if 8 in pf:
                g = np.frombuffer(pf[8][0], dtype="<f8").reshape(-1, 3)
                grf_list.append(g[:n_contacts])
            else:
                grf_list.append(np.zeros((n_contacts, 3), dtype=np.float64))
        off += tlen * frame_sz
        if len(jcs_list) != tlen:
            continue

        jcs = np.stack(jcs_list, axis=0)
        grf = np.stack(grf_list, axis=0)
        n_joints = jcs.shape[1]
        if len(osim_joints) == n_joints:
            jnames = list(osim_joints)
        else:
            no_pat = [n for n in osim_joints if not n.startswith("patellofemoral")]
            jnames = no_pat if len(no_pat) == n_joints else [f"joint_{i}" for i in range(n_joints)]

        trials.append(
            B3dTrial(
                name=tname,
                fps=1.0 / dt,
                joints=jnames,
                joint_centers=jcs,
                contact_bodies=contact_bodies,
                grf=grf,
            )
        )
    return trials


def load(path: str | pathlib.Path) -> list[B3dTrial]:
    """Load all trials from a ``.b3d`` file on disk."""
    return load_bytes(pathlib.Path(path).read_bytes())


def landmarks(trial: B3dTrial) -> tuple[dict[str, np.ndarray], float]:
    """Convert a ``B3dTrial``'s 20 Rajagopal OpenSim joint centers (Y up) into the 17 rig landmarks
    (Z up, facing -Y). Synthesizes ``neck`` and ``head`` from the rigid torso frame (``back`` ->
    mid-``acromial``)."""
    # Y-up -> Z-up (proper rotation: x' = x, y' = -z, z' = y)
    p = trial.joint_centers
    pts = np.stack([p[..., 0], -p[..., 2], p[..., 1]], axis=-1)
    jmap = {name: pts[:, idx] for idx, name in enumerate(trial.joints)}
    required = (
        "hip_l", "hip_r", "walker_knee_l", "walker_knee_r", "ankle_l", "ankle_r",
        "mtp_l", "mtp_r", "back", "acromial_l", "acromial_r", "elbow_l", "elbow_r",
        "radius_hand_l", "radius_hand_r",
    )
    missing = [k for k in required if k not in jmap]
    if missing:
        raise ValueError(f"b3d trial {trial.name!r} missing required OpenSim joints: {missing}")

    mid_sh = 0.5 * (jmap["acromial_l"] + jmap["acromial_r"])
    up_vec = mid_sh - jmap["back"]
    neck = mid_sh + 0.15 * up_vec
    head = mid_sh + 0.55 * up_vec
    hips = 0.5 * (jmap["hip_l"] + jmap["hip_r"])

    src = {
        "hips": hips,
        "spine": jmap["back"],
        "neck": neck,
        "head": head,
        "l_shoulder": jmap["acromial_l"],
        "l_elbow": jmap["elbow_l"],
        "l_wrist": jmap["radius_hand_l"],
        "r_shoulder": jmap["acromial_r"],
        "r_elbow": jmap["elbow_r"],
        "r_wrist": jmap["radius_hand_r"],
        "l_hip": jmap["hip_l"],
        "l_knee": jmap["walker_knee_l"],
        "l_ankle": jmap["ankle_l"],
        "l_toe": jmap["mtp_l"],
        "r_hip": jmap["hip_r"],
        "r_knee": jmap["walker_knee_r"],
        "r_ankle": jmap["ankle_r"],
        "r_toe": jmap["mtp_r"],
    }
    return src, trial.fps


def _fetch_range(url: str, start: int, length: int) -> bytes:
    req = urllib.request.Request(url, headers={"Range": f"bytes={start}-{start + length - 1}"})
    with urllib.request.urlopen(req, timeout=120) as resp:
        return resp.read()


def list_remote_entries(url: str = ARCHIVE_URL) -> list[tuple[str, int, int, int, int]]:
    """Read the ZIP64 central directory via HTTP Range and return
    ``(name, comp_method, comp_size, uncomp_size, local_header_offset)`` for every ``.b3d``."""
    req = urllib.request.Request(url, method="HEAD")
    with urllib.request.urlopen(req, timeout=30) as resp:
        total_size = int(resp.headers["Content-Length"])

    tail = _fetch_range(url, total_size - 65536, 65536)
    eocd_pos = tail.rfind(b"PK\x05\x06")
    z64_loc = tail.rfind(b"PK\x06\x07", 0, eocd_pos)
    z64_offset = struct.unpack_from("<Q", tail, z64_loc + 8)[0]
    z64_eocd = _fetch_range(url, z64_offset, 56)
    cd_size, cd_offset = struct.unpack_from("<QQ", z64_eocd, 40)
    cd_bytes = _fetch_range(url, cd_offset, cd_size)

    entries: list[tuple[str, int, int, int, int]] = []
    i = 0
    while True:
        j = cd_bytes.find(b"PK\x01\x02", i)
        if j < 0 or j + 46 > len(cd_bytes):
            break
        comp_method = int.from_bytes(cd_bytes[j + 10 : j + 12], "little")
        csize = int.from_bytes(cd_bytes[j + 20 : j + 24], "little")
        usize = int.from_bytes(cd_bytes[j + 24 : j + 28], "little")
        nlen = int.from_bytes(cd_bytes[j + 28 : j + 30], "little")
        elen = int.from_bytes(cd_bytes[j + 30 : j + 32], "little")
        clen = int.from_bytes(cd_bytes[j + 32 : j + 34], "little")
        lhoff = int.from_bytes(cd_bytes[j + 42 : j + 46], "little")
        name = cd_bytes[j + 46 : j + 46 + nlen].decode("utf-8", "replace")
        extra = cd_bytes[j + 46 + nlen : j + 46 + nlen + elen]
        k = 0
        while k + 4 <= len(extra):
            eid = int.from_bytes(extra[k : k + 2], "little")
            esz = int.from_bytes(extra[k + 2 : k + 4], "little")
            edata = extra[k + 4 : k + 4 + esz]
            if eid == 1:
                pos = 0
                if usize == 0xFFFFFFFF and pos + 8 <= len(edata):
                    usize = int.from_bytes(edata[pos : pos + 8], "little")
                    pos += 8
                if csize == 0xFFFFFFFF and pos + 8 <= len(edata):
                    csize = int.from_bytes(edata[pos : pos + 8], "little")
                    pos += 8
                if lhoff == 0xFFFFFFFF and pos + 8 <= len(edata):
                    lhoff = int.from_bytes(edata[pos : pos + 8], "little")
                    pos += 8
            k += 4 + esz
        if name.endswith(".b3d") and csize > 0:
            entries.append((name, comp_method, csize, usize, lhoff))
        i = j + 46 + nlen + elen + clen
    return entries


def fetch(
    out_dir: pathlib.Path = RAW_DIR,
    studies: tuple[str, ...] = OVERGROUND_STUDIES,
    max_files_per_study: int | None = None,
    url: str = ARCHIVE_URL,
) -> list[pathlib.Path]:
    """Download ``.b3d`` files for the selected ``With_Arm`` studies via HTTP Range requests."""
    entries = list_remote_entries(url)
    by_study: dict[str, list[tuple[str, int, int, int, int]]] = {}
    for e in entries:
        parts = e[0].split("/")
        if len(parts) >= 3 and parts[1] == "With_Arm" and parts[2] in studies:
            by_study.setdefault(parts[2], []).append(e)

    written: list[pathlib.Path] = []
    for study in studies:
        study_entries = sorted(by_study.get(study, []), key=lambda x: x[2])
        if max_files_per_study is not None:
            study_entries = study_entries[:max_files_per_study]
        for name, comp_method, csize, usize, lhoff in study_entries:
            fname = name.split("/")[-1]
            dst = out_dir / study / fname
            if dst.exists() and dst.stat().st_size == usize:
                written.append(dst)
                continue
            dst.parent.mkdir(parents=True, exist_ok=True)
            print(f"  fetching {study}/{fname} ({csize / 1e6:.1f} MB compressed)...", flush=True)
            chunk = _fetch_range(url, lhoff, csize + 512)
            nlen = int.from_bytes(chunk[26:28], "little")
            elen = int.from_bytes(chunk[28:30], "little")
            payload = chunk[30 + nlen + elen : 30 + nlen + elen + csize]
            raw = zlib.decompress(payload, -15) if comp_method == 8 else payload
            tmp = dst.with_suffix(".b3d.part")
            tmp.write_bytes(raw)
            tmp.replace(dst)
            written.append(dst)
    return written


def main(argv=None):
    ap = argparse.ArgumentParser(description="Fetch overground With_Arm .b3d files from AddBiomechanics 1.0")
    ap.add_argument("--out", default=str(RAW_DIR), help="destination folder (default: training/data/raw/addbiomechanics)")
    ap.add_argument("--studies", default=",".join(OVERGROUND_STUDIES), help="comma-separated study names")
    ap.add_argument("--max-files-per-study", type=int, help="download at most N smallest files per study")
    args = ap.parse_args(argv)
    studies = tuple(s.strip() for s in args.studies.split(",") if s.strip())
    paths = fetch(pathlib.Path(args.out), studies=studies, max_files_per_study=args.max_files_per_study)
    print(f"addbiomechanics: {len(paths)} .b3d files ready in {args.out}")


if __name__ == "__main__":
    main()

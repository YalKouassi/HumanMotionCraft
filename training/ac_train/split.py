"""Which takes validation holds out: whole subjects (or sessions), never single takes.

Near-identical takes of one person (the same subject walking twice, a generated prompt's seeds)
would sit on both sides of a per-take split, and the validation numbers would flatter the model.
So each take belongs to a group (:func:`group_of`), and a group is held out or not as a whole:

- CMU: the subject (``walk_cmu_02_01`` -> ``02``);
- ACCAD: the recording session, a performer's lettered series (``male2_e``: Male2's E takes, all
  fights; ``female1_b``: Female1's walks); a take outside a series is its own group. ACCAD has only
  three performers, so holding out whole performers held out none of them (owner, 2026-10-30): a
  session is held out instead. The same person is on both sides, so ACCAD's numbers are a little
  kinder than the other sets'.
- AddBiomechanics: the study and subject (``falisse2017_subject_0``, ``han2023_s001``);
- generated clips (``kimodo``): the prompt, every seed together (``_s1``, ``_s2`` ... dropped);
- 100STYLE is one performer and the built-in loops are hand-made: each take is its own group.

A group is held out when a hash of ``<set>/<group>`` falls under :data:`SHARE` (:func:`held_out`).
That depends on nothing but the names, so the split stays the same when a set is added, a take is
dropped by the review or the sets are read in another order.
"""

from __future__ import annotations

import re
import zlib

#: about this share of each set's groups is held out
SHARE = 0.05


def group_of(set_name: str, take: str) -> str:
    """The subject or session a take belongs to (see the module doc)."""
    t = str(take)
    if set_name == "cmu":
        m = re.search(r"_cmu_(\d+)_\d+$", t)
        return m.group(1) if m else t
    if set_name == "accad":
        m = re.search(r"_accad_((?:fe)?male\d+)_([a-z])\d+", t)
        return f"{m.group(1)}_{m.group(2)}" if m else t
    if set_name == "addbiomechanics":
        rest = t.split("_addbiomechanics_", 1)[-1].split("_")
        if len(rest) < 2:
            return t
        study, who = rest[0], rest[1]
        if who == "subject" and len(rest) > 2:  # "subject_0"
            who = f"subject_{rest[2]}"
        return f"{study}_{who}"
    if set_name == "kimodo":
        return re.sub(r"_s\d+$", "", t)
    return t


def score(set_name: str, group: str) -> float:
    """A stable number in [0, 1) for a group (CRC-32 of ``<set>/<group>``)."""
    return zlib.crc32(f"{set_name}/{group}".encode()) / 2**32


def held_out(set_name: str, take: str) -> bool:
    return score(set_name, group_of(set_name, take)) < SHARE


def holdout(takes: dict[str, list[str]]) -> dict[str, list[str]]:
    """{set: its take names} -> {set: the groups held out}, each set on its own."""
    return {n: sorted({group_of(n, t) for t in ts if held_out(n, t)}) for n, ts in takes.items()}

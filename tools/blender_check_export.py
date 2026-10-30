"""Load an exported .glb in Blender, headless, and render fixed views of a few frames.

Part of the engine check of the export (roadmap 122000). Our own code: it uses only Blender's
Python API (bpy) and its bundled glTF importer as a black box.

    blender --background --python tools/blender_check_export.py -- <glb> <out_prefix> <frames> [engine]

- `<glb>`: the file to import.
- `<out_prefix>`: PNGs are written as `<out_prefix>-<view>-<frame>.png`, `view` being `front`
  (camera on -Y looking +Y, the rig's own front) and `left` (camera on +X, the body's left side).
- `<frames>`: comma list of the clip's frames, e.g. `0,8,15`. Blender's importer puts glTF time 0
  on its own first frame; the script finds that frame from the action and offsets by it, so the
  clip's frame `f` is rendered, not Blender's frame `f`.
- `[engine]`: `workbench` (default, needs no GPU) or `eevee`.

Prints one line starting with `BLENDER_CHECK ` followed by JSON: bone count and names, the
action's frame range and the scene fps, the root bone's world position and the lowest mesh vertex
at each rendered frame (root motion, feet on the floor), the importer's warnings, and the files
written. Exits non-zero if the import fails or no armature comes out of it.
"""

import json
import math
import os
import sys
import tempfile

import bpy
from mathutils import Vector

FPS = 30
SIZE = (400, 500)
WARN_WORDS = ("warn", "error", "fail", "unsupported", "skipp", "ignor", "invalid")


def args():
    try:
        rest = sys.argv[sys.argv.index("--") + 1 :]
    except ValueError:
        rest = []
    if len(rest) < 3:
        print(__doc__)
        sys.exit(2)
    glb, prefix, frames = rest[0], rest[1], [int(x) for x in rest[2].split(",") if x]
    engine = rest[3] if len(rest) > 3 else "workbench"
    return glb, prefix, frames, engine


def import_glb(path):
    """Import, with everything Blender prints on its C-level stdout/stderr captured."""
    sys.stdout.flush()
    sys.stderr.flush()
    saved = (os.dup(1), os.dup(2))
    with tempfile.TemporaryFile(mode="w+b") as log:
        os.dup2(log.fileno(), 1)
        os.dup2(log.fileno(), 2)
        error = None
        try:
            bpy.ops.import_scene.gltf(filepath=path)
        except Exception as e:  # the importer raising is a finding, not a crash
            error = str(e)
        finally:
            sys.stdout.flush()
            sys.stderr.flush()
            os.dup2(saved[0], 1)
            os.dup2(saved[1], 2)
            os.close(saved[0])
            os.close(saved[1])
        log.seek(0)
        text = log.read().decode("utf-8", "replace")
    lines = [l.strip() for l in text.splitlines() if l.strip()]
    warnings = [l for l in lines if any(w in l.lower() for w in WARN_WORDS)]
    return warnings, lines, error


def look_at(cam, eye, target):
    cam.location = Vector(eye)
    direction = Vector(target) - Vector(eye)
    cam.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()


def main():
    glb, prefix, frames, engine = args()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.fps = FPS
    scene.render.fps_base = 1.0

    warnings, log, error = import_glb(glb)
    if error:
        print("BLENDER_CHECK " + json.dumps({"file": glb, "error": error, "log": log}))
        sys.exit(1)

    arms = [o for o in bpy.data.objects if o.type == "ARMATURE"]
    # the skinned meshes only: the importer also makes a bone-shape mesh of its own (an icosphere)
    meshes = [o for o in bpy.data.objects if o.type == "MESH" and any(m.type == "ARMATURE" for m in o.modifiers)]
    others = [o.name for o in bpy.data.objects if o.type == "MESH" and o not in meshes]
    if not arms:
        print("BLENDER_CHECK " + json.dumps({"file": glb, "error": "no armature imported", "log": log}))
        sys.exit(1)
    arm = arms[0]
    bones = [b.name for b in arm.data.bones]

    action = None
    if arm.animation_data and arm.animation_data.action:
        action = arm.animation_data.action
    elif bpy.data.actions:
        action = bpy.data.actions[0]
        arm.animation_data_create()
        arm.animation_data.action = action
    if action is None:
        print("BLENDER_CHECK " + json.dumps({"file": glb, "error": "no action imported", "bones": len(bones), "log": log}))
        sys.exit(1)
    f0, f1 = action.frame_range
    first = int(round(f0))

    # a floor for the feet to stand on
    bpy.ops.mesh.primitive_plane_add(size=12.0, location=(0.0, 0.0, 0.0))
    floor = bpy.context.active_object
    floor.name = "floor"

    cam_data = bpy.data.cameras.new("cam")
    cam_data.lens = 50.0
    cam = bpy.data.objects.new("cam", cam_data)
    scene.collection.objects.link(cam)
    scene.camera = cam
    views = {
        "front": ((0.0, -4.5, 1.0), (0.0, 0.0, 0.9)),
        "left": ((4.5, 0.0, 1.0), (0.0, 0.0, 0.9)),
    }

    scene.render.resolution_x, scene.render.resolution_y = SIZE
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    if engine == "eevee":
        scene.render.engine = "BLENDER_EEVEE_NEXT" if hasattr(bpy.types, "SceneEEVEE") else "BLENDER_EEVEE"
    else:
        scene.render.engine = "BLENDER_WORKBENCH"
        sh = scene.display.shading
        sh.light = "STUDIO"
        sh.color_type = "SINGLE"
        sh.single_color = (0.6, 0.6, 0.6)
        sh.show_shadows = True
        sh.show_cavity = True
        sh.show_object_outline = True
    scene.display_settings.display_device = "sRGB"
    try:
        scene.view_settings.view_transform = "Standard"
    except TypeError:
        pass

    depsgraph = bpy.context.evaluated_depsgraph_get()
    root_bone = arm.pose.bones.get("root") or arm.pose.bones[0]
    samples = []
    written = []
    for f in frames:
        scene.frame_set(first + f)
        depsgraph.update()
        root_world = arm.matrix_world @ root_bone.head
        lowest = None
        for m in meshes:
            ev = m.evaluated_get(depsgraph)
            mw = ev.matrix_world
            for v in ev.data.vertices:
                z = (mw @ v.co).z
                if lowest is None or z < lowest:
                    lowest = z
        samples.append(
            {
                "frame": f,
                "blender_frame": first + f,
                "root": [round(c, 4) for c in root_world],
                "lowest_vertex_z": None if lowest is None else round(lowest, 4),
            }
        )
        for view, (eye, target) in views.items():
            look_at(cam, eye, target)
            out = f"{prefix}-{view}-{f}.png"
            scene.render.filepath = out
            bpy.ops.render.render(write_still=True)
            written.append(out)

    report = {
        "file": glb,
        "blender": bpy.app.version_string,
        "engine": scene.render.engine,
        "armature": arm.name,
        "bones": len(bones),
        "bone_names": bones,
        "meshes": [m.name for m in meshes],
        "other_mesh_objects": others,
        "vertices": sum(len(m.data.vertices) for m in meshes),
        "action": action.name,
        "frame_range": [f0, f1],
        "frames_in_action": int(round(f1 - f0)) + 1,
        "fps": scene.render.fps / scene.render.fps_base,
        "samples": samples,
        "import_warnings": warnings,
        "written": written,
    }
    print("BLENDER_CHECK " + json.dumps(report))


main()

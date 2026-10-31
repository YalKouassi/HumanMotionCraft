"""Import an FBX in Blender, headless, and print its joints' world positions frame by frame.

An outside check of FBX files (the test files `ac_train.fbxwrite` writes, and files to import)
through Blender's own importer as a black box. Our own code: it uses only Blender's Python API.

    blender --background --python tools/blender_fbx_joints.py -- <fbx> <out.json> [frames] [render_prefix]

- `<fbx>`: the file to import.
- `<out.json>`: written with the scene fps, the action's frame range, every bone's name and parent,
  and `positions`: per frame, per bone, the bone head's world position in Blender's axes (Z up,
  metres: Blender applies the file's unit scale and axis system on import).
- `[frames]`: comma list of frames to sample (default: the action's whole range).
- `[render_prefix]`: when given, the sampled frames are also rendered (workbench, no GPU) from the
  front (camera on -Y) as `<render_prefix>-<frame>.png`, bones shown as sticks.

Exits non-zero when the import fails or no armature comes out of it.
"""

import json
import math
import sys

import bpy
from mathutils import Vector

SIZE = (400, 500)


def args():
    try:
        rest = sys.argv[sys.argv.index("--") + 1 :]
    except ValueError:
        rest = []
    if len(rest) < 2:
        print(__doc__)
        sys.exit(2)
    frames = [int(x) for x in rest[2].split(",") if x] if len(rest) > 2 and rest[2] else None
    prefix = rest[3] if len(rest) > 3 else None
    return rest[0], rest[1], frames, prefix


def armature():
    arms = [o for o in bpy.data.objects if o.type == "ARMATURE"]
    if not arms:
        print("no armature imported")
        sys.exit(1)
    return arms[0]


def frame_range(obj):
    anim = obj.animation_data
    if anim and anim.action:
        a, b = anim.action.frame_range
        return int(round(a)), int(round(b))
    return bpy.context.scene.frame_start, bpy.context.scene.frame_end


def look_at(cam, eye, target):
    cam.location = Vector(eye)
    direction = Vector(target) - Vector(eye)
    cam.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()


def setup_render(obj):
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.render.resolution_x, scene.render.resolution_y = SIZE
    scene.render.image_settings.file_format = "PNG"
    scene.display.shading.light = "FLAT"
    scene.display.shading.color_type = "SINGLE"
    scene.display.shading.single_color = (0.85, 0.85, 0.85)
    scene.display.shading.background_type = "VIEWPORT"
    scene.display.shading.background_color = (0.12, 0.12, 0.14)
    obj.data.display_type = "STICK"
    obj.show_in_front = True
    cam_data = bpy.data.cameras.new("cam")
    cam = bpy.data.objects.new("cam", cam_data)
    scene.collection.objects.link(cam)
    scene.camera = cam
    cam_data.lens = 35
    return cam


def main():
    path, out, frames, prefix = args()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    try:
        bpy.ops.import_scene.fbx(filepath=path)
    except Exception as e:  # the importer raising is a finding, not a crash
        print(f"import failed: {e}")
        sys.exit(1)
    obj = armature()
    scene = bpy.context.scene
    a, b = frame_range(obj)
    if frames is None:
        frames = list(range(a, b + 1))
    bones = [(pb.name, pb.parent.name if pb.parent else None) for pb in obj.pose.bones]
    positions = []
    cam = setup_render(obj) if prefix else None
    for f in frames:
        scene.frame_set(f)
        bpy.context.view_layer.update()
        row = []
        for pb in obj.pose.bones:
            p = obj.matrix_world @ pb.head
            row.append([p.x, p.y, p.z])
        positions.append(row)
        if cam is not None:
            # the camera looks along +Y at the body's hips height from 4 m in front
            hips = Vector(row[0]) if row else Vector((0, 0, 1))
            look_at(cam, (0.0, -4.0, 1.0), (0.0, 0.0, hips.z))
            scene.render.filepath = f"{prefix}-{f}.png"
            bpy.ops.render.render(write_still=True)
    result = {
        "file": path,
        "fps": scene.render.fps / scene.render.fps_base,
        "range": [a, b],
        "frames": frames,
        "bones": [n for n, _ in bones],
        "parents": [p for _, p in bones],
        "positions": positions,
        "blender": bpy.app.version_string,
        "armature_parent": obj.parent.name if obj.parent else None,
        "armature_matrix_world": [list(r) for r in obj.matrix_world],
    }
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(result, fh)
    print("BLENDER_FBX " + json.dumps({"bones": len(bones), "frames": len(frames), "range": [a, b], "fps": result["fps"]}))
    if math.isnan(sum(sum(p) for row in positions for p in row)):
        sys.exit(1)


main()

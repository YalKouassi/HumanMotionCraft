"""Drive the app's window the way a person does and check what happens.

Starts the release app on a scratch library with a control port, plays pointer and key input
through it (box-select, dragging keys, a refused move, click-select, Delete, Ctrl+Z, snapping
with and without Alt, wheel zoom, right-click pinning, the Reach and Move tools; the viewport
keys Q W E R, the Rotate gizmo's rings in world and local axes with Ctrl snapping, onion skin,
trails, the camera keys 1 3 7 5 0 F and follow, Shift+click and the picker; the timeline's
key menu, keyboard navigation, copy / paste, retime, dope-sheet rows, markers, the play range,
problem marks, curves and the foot-contact strip; context keys; op click by widget id), checks the agent protocol (request ids,
events since a cursor, an event subscription, the animation op), checks each result, and
closes the app. Exit 1 when a check fails.

    python tools/drive_ui.py [--port 7812] [--shots <dir>] [--exe <app>]

Build first: cargo build --release -p ac-app.
"""
import argparse, atexit, base64, json, os, socket, subprocess, sys, tempfile, time, tomllib

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCENE = [
    {"command": "pose.set_root", "params": {"root": [0, 0, 0.9450101]}},
    {"command": "key.set", "params": {"interp": "linear"}},
    {"command": "frame.set", "params": {"frame": 15}},
    {"command": "pose.set_root", "params": {"root": [0, 0, 1.9450101]}},
    {"command": "key.set", "params": {"interp": "linear"}},
    {"command": "frame.set", "params": {"frame": 30}},
    {"command": "pose.set_root", "params": {"root": [0, 0, 0.9450101]}},
    {"command": "key.set", "params": {"interp": "linear"}},
    {"command": "frame.set", "params": {"frame": 60}},
    {"command": "pose.set_root", "params": {"root": [1, 0, 0.9450101]}},
    {"command": "key.set", "params": {"interp": "linear"}},
    {"command": "physics.set", "params": {"on": True}},
    {"command": "pin.set", "params": {"bone": "wrist.L", "on": True}},
    {"command": "frame.set", "params": {"frame": 12}},
]

ap = argparse.ArgumentParser()
ap.add_argument('--port', type=int, default=7812)
ap.add_argument('--shots', default=None)
ap.add_argument('--exe', default=None, help='the app to drive (default: target/release; another build while the app is open)')
a = ap.parse_args()
PORT, OUT = a.port, a.shots
work = tempfile.mkdtemp(prefix='ac-drive-')
scene = os.path.join(work, 'scene.json')
json.dump(SCENE, open(scene, 'w'))
# the binary's name comes from ac-app's Cargo.toml, so a rename there is the only edit
with open(os.path.join(REPO, 'crates', 'ac-app', 'Cargo.toml'), 'rb') as f:
    APP_BIN = tomllib.load(f)['bin'][0]['name']
exe = os.path.abspath(a.exe) if a.exe else os.path.join(REPO, 'target', 'release', APP_BIN + ('.exe' if os.name == 'nt' else ''))
app = subprocess.Popen([exe, '--library', os.path.join(work, 'library'), '--script', scene, '--control', str(PORT)],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
# whatever happens below (a failed request, a script error), the app goes with the script
atexit.register(lambda: app.poll() is None and app.kill())

for _ in range(120):
    try:
        s = socket.create_connection(('127.0.0.1', PORT))
        break
    except OSError:
        if app.poll() is not None:
            raise SystemExit(f'the app exited ({app.returncode})')
        time.sleep(0.5)
else:
    app.kill()
    raise SystemExit('no control port')
f = s.makefile('rw', encoding='utf-8')

def req(msg):
    f.write(json.dumps(msg) + '\n'); f.flush()
    r = json.loads(f.readline())
    if 'error' in r:
        app.kill()
        raise SystemExit(f"{msg.get('op')}: {r['error']}")
    return r['ok']


def run(cmd, params=None):
    return req({'op': 'run', 'command': cmd, 'params': params or {}})


def keys():
    return [k['frame'] for k in run('editor.state')['keys']]


def shot(name):
    if OUT:
        png = base64.b64decode(req({'op': 'screenshot'})['png'])
        os.makedirs(OUT, exist_ok=True)
        open(os.path.join(OUT, f'{name}.png'), 'wb').write(png)


def drag(x0, y0, x1, y1, button='primary', mods=(), steps=8):
    ev = [{'kind': 'down', 'x': x0, 'y': y0, 'button': button, 'modifiers': list(mods)}]
    for i in range(1, steps + 1):
        t = i / steps
        ev.append({'kind': 'move', 'x': x0 + (x1 - x0) * t, 'y': y0 + (y1 - y0) * t, 'modifiers': list(mods)})
    ev.append({'kind': 'up', 'x': x1, 'y': y1, 'button': button, 'modifiers': list(mods)})
    return req({'op': 'input', 'events': ev})


checks = []


def check(name, ok, detail=''):
    checks.append((name, ok))
    print(('ok    ' if ok else 'FAIL  ') + name + (f'  ({detail})' if detail else ''))


# let it settle, then read the layout
req({'op': 'input', 'events': [{'kind': 'move', 'x': 700, 'y': 400}]})
lay = req({'op': 'layout'})
tl = lay['timeline']
fx = tl['frame_x']
x = lambda frame: fx['origin'] + (frame - fx['start']) * fx['points_per_frame']
ky = tl['keys_y']
check('start keys', keys() == [0, 15, 30, 60], keys())

# box-select from empty frame 12 to 33: keys 15 and 30
r = drag(x(12), ky - 4, x(33), ky + 4)
check('box-select 15 and 30', r['selected_keys'] == [15, 30], r['selected_keys'])

# drag key 30 by +10: both selected keys move
r = drag(x(30), ky, x(40), ky)
check('drag moves the selection', keys() == [0, 25, 40, 60], keys())
# the undo readout is in the timeline header; ask the session through edit.undo's reply after
# checking the frames, then redo
u = run('edit.undo')
check('undo says what it undoes', u.get('label') == 'Move 2 keys by 10', u)
check('undo puts them back', keys() == [0, 15, 30, 60], keys())
run('edit.redo')
check('redo moves them again', keys() == [0, 25, 40, 60], keys())

# a move onto a key that stays is refused whole
r = drag(x(25), ky, x(60), ky)
check('refused move changes nothing', keys() == [0, 25, 40, 60], f"{keys()} status={r['status']!r}")

# click a key: selects it and moves the playhead there; Delete removes it
r = req({'op': 'input', 'events': [{'kind': 'click', 'x': x(25), 'y': ky}]})
check('click selects one key', r['selected_keys'] == [25] and r['frame'] == 25, (r['selected_keys'], r['frame']))
r = req({'op': 'input', 'events': [{'kind': 'move', 'x': 700, 'y': 300}, {'kind': 'key', 'key': 'Delete'}]})
check('Delete removes the selected key', keys() == [0, 40, 60], keys())
r = req({'op': 'input', 'events': [{'kind': 'key', 'key': 'Z', 'modifiers': ['ctrl']}]})
check('Ctrl+Z brings it back', keys() == [0, 25, 40, 60], keys())

# zoomed out, a click 6 points short of key 40 lands on frame 39 by rounding: snapping puts the
# playhead on the key; with Alt it stays on 39
req({'op': 'input', 'events': [{'kind': 'scroll', 'x': x(30), 'y': ky, 'dy': -450}] + [{'kind': 'move', 'x': x(30), 'y': ky}] * 6})
fx = req({'op': 'layout'})['timeline']['frame_x']
near = x(40) - 6
check('zoomed out for the snap test', fx['points_per_frame'] < 10, fx['points_per_frame'])
r = req({'op': 'input', 'events': [{'kind': 'click', 'x': near, 'y': tl['ruler_y']}]})
check('scrub snaps to a key', r['frame'] == 40, r['frame'])
plain = round(40 - 6 / fx['points_per_frame'])
r = req({'op': 'input', 'events': [{'kind': 'click', 'x': near, 'y': tl['ruler_y'], 'modifiers': ['alt']}]})
check('Alt scrubs without snapping', plain != 40 and r['frame'] == plain, f"{r['frame']} (unsnapped {plain})")
fx = req({'op': 'layout'})['timeline']['frame_x']

# wheel over the timeline zooms in
before = fx['points_per_frame']
req({'op': 'input', 'events': [{'kind': 'scroll', 'x': x(30), 'y': ky, 'dy': 120}] + [{'kind': 'move', 'x': x(30), 'y': ky}] * 6})
after = req({'op': 'layout'})['timeline']['frame_x']['points_per_frame']
check('wheel zooms the timeline', after > before * 1.2, f'{before:.2f} -> {after:.2f}')

# right-click a joint: pins it; again: unpins
run('frame.set', {'frame': 0})
req({'op': 'input', 'events': [{'kind': 'move', 'x': 700, 'y': 400}]})
lay = req({'op': 'layout'})
jx, jy = lay['joints']['wrist.R']
req({'op': 'input', 'events': [{'kind': 'click', 'x': jx, 'y': jy, 'button': 'secondary'}]})
pins = run('editor.state')['pins']
check('right-click pins the joint', 'wrist.R' in pins, pins)
shot('live-pinned')
req({'op': 'input', 'events': [{'kind': 'click', 'x': jx, 'y': jy, 'button': 'secondary'}]})
pins = run('editor.state')['pins']
check('right-click again unpins it', 'wrist.R' not in pins, pins)

# reach tool: dragging the right wrist up moves it, one undo step named Reach
req({'op': 'ui', 'name': 'tool', 'value': 'reach'})
w0 = run('bone.world', {'bone': 'wrist.R'})['position']
drag(jx, jy, jx + 10, jy - 80, steps=12)
w1 = run('bone.world', {'bone': 'wrist.R'})['position']
check('reach drag lifts the wrist', w1[2] > w0[2] + 0.1, f'z {w0[2]:.3f} -> {w1[2]:.3f}')
u = run('edit.undo')
check('reach is one undo step named Reach', u.get('label') == 'Reach', u)

# move tool: a drag slides the body on this frame (and its key) over the floor, every pose as it
# was, other keys where they were; one undo step. Alt+drag moves the whole animation. Shift+drag
# turns about the root
req({'op': 'ui', 'name': 'tool', 'value': 'move'})
world = lambda b: run('bone.world', {'bone': b})['position']
sub = lambda p, q: [a - b for a, b in zip(p, q)]
run('frame.set', {'frame': 60})
end0 = world('root')
run('frame.set', {'frame': 0})
req({'op': 'input', 'events': [{'kind': 'move', 'x': 700, 'y': 400}]})
rx, ry = req({'op': 'layout'})['joints']['root']
r0, f0 = world('root'), world('foot.L')
drag(rx, ry, rx + 120, ry + 30, steps=12)
r1, f1 = world('root'), world('foot.L')
slid = sub(r1, r0)
check('move drag slides the body', (slid[0] ** 2 + slid[1] ** 2) ** 0.5 > 0.1 and abs(slid[2]) < 1e-6, slid)
check('move keeps the pose', max(abs(a - b) for a, b in zip(sub(f1, r1), sub(f0, r0))) < 1e-6, (sub(f0, r0), sub(f1, r1)))
check('move keeps the keys', keys() == [0, 25, 40, 60], keys())
run('frame.set', {'frame': 25})
run('frame.set', {'frame': 0})
check('the moved key stays moved', max(abs(a - b) for a, b in zip(world('root'), r1)) < 1e-6, (world('root'), r1))
run('frame.set', {'frame': 60})
check('move leaves the other keys', max(abs(a - b) for a, b in zip(world('root'), end0)) < 1e-6, (world('root'), end0))
u = run('edit.undo')
check('move is one undo step named Move body on this frame', u.get('label') == 'Move body on this frame', u)
run('frame.set', {'frame': 0})
req({'op': 'input', 'events': [{'kind': 'move', 'x': 700, 'y': 400}]})
rx, ry = req({'op': 'layout'})['joints']['root']
drag(rx, ry, rx + 120, ry + 30, mods=('alt',), steps=12)
slid = sub(world('root'), r0)
run('frame.set', {'frame': 60})
end1 = world('root')
check('Alt+drag takes every key along', max(abs(a - b) for a, b in zip(sub(end1, end0), slid)) < 1e-6 and (slid[0] ** 2 + slid[1] ** 2) ** 0.5 > 0.1, (sub(end1, end0), slid))
shot('live-moved')
u = run('edit.undo')
check('Alt+drag is one undo step named Move animation', u.get('label') == 'Move animation', u)
run('frame.set', {'frame': 0})
req({'op': 'input', 'events': [{'kind': 'move', 'x': 700, 'y': 400}]})
rx, ry = req({'op': 'layout'})['joints']['root']
drag(rx, ry, rx + 180, ry, mods=('shift',), steps=12)
q = run('bone.world', {'bone': 'root'})['rotation']
check('shift+drag turns about the root', abs(q[2]) > 0.5 and max(abs(a - b) for a, b in zip(world('root'), r0)) < 1e-4, (q, world('root'), r0))
shot('live-turned')
run('edit.undo')
shot('live-end')

# viewport keys (pointer over the viewport): Q W E R pick the tools
HOVER = {'kind': 'move', 'x': 700, 'y': 400}
for key, tool in [('E', 'rotate'), ('W', 'move'), ('R', 'reach'), ('Q', 'drag'), ('E', 'rotate')]:
    r = req({'op': 'input', 'events': [HOVER, {'kind': 'key', 'key': key}]})
    check(f'key {key} picks {tool}', r['layout'].get('tool') == tool, r['layout'].get('tool'))


def quat_mul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return [aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz]


def turn_between(q0, q1):
    """The world turn taking q0 to q1: unit axis and degrees."""
    d = quat_mul(q1, [-q0[0], -q0[1], -q0[2], q0[3]])
    if d[3] < 0:
        d = [-c for c in d]
    s = sum(c * c for c in d[:3]) ** 0.5
    import math
    return ([c / s for c in d[:3]] if s > 1e-9 else [0, 0, 0]), math.degrees(2 * math.atan2(s, d[3]))


def sweep(cx, cy, x0, y0, degrees, mods=(), steps=16):
    """Drag from (x0, y0) round the centre (cx, cy) by `degrees` on screen (counter-clockwise)."""
    import math
    r = math.hypot(x0 - cx, y0 - cy)
    a0 = math.atan2(-(y0 - cy), x0 - cx)
    ev = [{'kind': 'down', 'x': x0, 'y': y0, 'modifiers': list(mods)}]
    for i in range(1, steps + 1):
        a = a0 + math.radians(degrees) * i / steps
        ev.append({'kind': 'move', 'x': cx + r * math.cos(a), 'y': cy - r * math.sin(a), 'modifiers': list(mods)})
    ev.append({'kind': 'up', 'x': ev[-1]['x'], 'y': ev[-1]['y'], 'modifiers': list(mods)})
    return req({'op': 'input', 'events': ev})


# rotate tool: click a joint to select it; the gizmo appears round it
run('frame.set', {'frame': 0})
req({'op': 'input', 'events': [HOVER]})
lay = req({'op': 'layout'})
ex, ey = lay['joints']['lowerarm01.R']
r = req({'op': 'input', 'events': [{'kind': 'click', 'x': ex, 'y': ey}]})
g = r['layout'].get('gizmo')
check('click selects the joint and shows the gizmo', r['layout']['selection']['active'] == 'lowerarm01.R' and g is not None,
      r['layout']['selection'])
shot('live-gizmo')

# drag the Z ring round (seen from above, so it is a circle): the joint turns about world Z, its
# head stays, one undo step
req({'op': 'ui', 'name': 'camera', 'value': 'top'})
g = req({'op': 'input', 'events': [HOVER]})['layout']['gizmo']
elbow0 = run('bone.world', {'bone': 'lowerarm01.R'})
gx, gy = g['axes'][2]
cx, cy = g['centre']
sweep(cx, cy, gx, gy, 40)
elbow1 = run('bone.world', {'bone': 'lowerarm01.R'})
axis, deg = turn_between(elbow0['rotation'], elbow1['rotation'])
check('Z ring turns about world Z', abs(abs(axis[2]) - 1) < 1e-3 and deg > 5, (axis, deg))
check('the turned joint stays put', max(abs(a - b) for a, b in zip(elbow0['position'], elbow1['position'])) < 1e-6)
u = run('edit.undo')
check('a turn is one undo step named Rotate joint', u.get('label') == 'Rotate joint', u)
back = run('bone.world', {'bone': 'lowerarm01.R'})
check('undo turns it back', turn_between(elbow0['rotation'], back['rotation'])[1] < 1e-4)

# Ctrl snaps the view ring's turn to 15 degrees
vr = g['view_ring_px']
r = sweep(cx, cy, cx + vr, cy, 100, mods=('ctrl',))
import re
m = re.search(r'turned \S+ (-?[\d.]+)°', r['status'])
a = float(m.group(1)) if m else None
check('Ctrl snaps the turn to 15°', a is not None and abs(a) > 1 and abs(a / 15 - round(a / 15)) < 1e-3, r['status'])
shot('live-turned-snap')
run('edit.undo')

# local axes: the X ring turns about the joint's own X axis (seen from the side, face on)
req({'op': 'ui', 'name': 'gizmo', 'value': 'local'})
req({'op': 'ui', 'name': 'camera', 'value': 'side'})
lay = req({'op': 'input', 'events': [HOVER]})['layout']
g = lay['gizmo']
e0 = run('bone.world', {'bone': 'lowerarm01.R'})
sweep(g['centre'][0], g['centre'][1], g['axes'][0][0], g['axes'][0][1], 30)
e1 = run('bone.world', {'bone': 'lowerarm01.R'})
axis, deg = turn_between(e0['rotation'], e1['rotation'])
q = e0['rotation']
# the joint's own X axis in the world: the first column of its rotation matrix
qx, qy, qz, qw = q
own_x = [1 - 2 * (qy * qy + qz * qz), 2 * (qx * qy + qw * qz), 2 * (qx * qz - qw * qy)]
check('local X ring turns about the joint\'s own X', abs(abs(sum(a * b for a, b in zip(axis, own_x))) - 1) < 1e-3 and deg > 3, (axis, own_x, deg))
run('edit.undo')
req({'op': 'ui', 'name': 'gizmo', 'value': 'world'})

# onion skin: ghosts every 2 frames, 2 a side, then the nearest keys
run('frame.set', {'frame': 12})
req({'op': 'ui', 'name': 'onion', 'value': 'on'})
lay = req({'op': 'input', 'events': [HOVER]})['layout']
check('onion ghosts 2 frames apart', lay['onion'] == [10, 8, 14, 16], lay['onion'])
shot('live-onion')
req({'op': 'ui', 'name': 'onion', 'value': 'keys'})
lay = req({'op': 'input', 'events': [HOVER]})['layout']
check('onion ghosts on the nearest keys', lay['onion'] == [0, 25, 40], (lay['onion'], keys()))
req({'op': 'ui', 'name': 'onion', 'value': 'off'})
lay = req({'op': 'input', 'events': [HOVER]})['layout']
check('onion off: no ghosts', lay['onion'] == [], lay['onion'])

# trails: the selected joint's path over the whole animation, with its keys
req({'op': 'ui', 'name': 'trails', 'value': 'on'})
req({'op': 'ui', 'name': 'select', 'value': 'root'})
lay = req({'op': 'input', 'events': [HOVER]})['layout']
t = lay.get('trail') or {}
check('trail covers the animation and marks its keys', t.get('frames') == 61 and t.get('keys') == keys(), t)
shot('live-trail')
req({'op': 'ui', 'name': 'trails', 'value': 'off'})

# camera keys: 1 front, Ctrl+1 back, 7 top, 5 orthographic, F frames the selection
cam = lambda r: r['layout']['camera']
r = req({'op': 'input', 'events': [HOVER, {'kind': 'key', 'key': '1'}]})
check('1 looks from the front', abs(cam(r)['yaw']) < 1e-6 and abs(cam(r)['pitch'] - 0.1) < 1e-6, cam(r))
r = req({'op': 'input', 'events': [HOVER, {'kind': 'key', 'key': '1', 'modifiers': ['ctrl']}]})
check('Ctrl+1 looks from the back', abs(cam(r)['yaw'] - 3.14159265) < 1e-5, cam(r))
r = req({'op': 'input', 'events': [HOVER, {'kind': 'key', 'key': '7'}]})
check('7 looks from the top', abs(cam(r)['pitch'] - 1.45) < 1e-6, cam(r))
r = req({'op': 'input', 'events': [HOVER, {'kind': 'key', 'key': '5'}]})
check('5 turns orthographic on', cam(r)['ortho'] is True, cam(r))
shot('live-ortho-top')
r = req({'op': 'input', 'events': [HOVER, {'kind': 'key', 'key': '5'}, {'kind': 'key', 'key': '0'}]})
check('5 again: perspective; 0: three-quarter', cam(r)['ortho'] is False and abs(cam(r)['yaw'] - 0.6) < 1e-6, cam(r))
req({'op': 'ui', 'name': 'select', 'value': 'wrist.L'})
r = req({'op': 'input', 'events': [HOVER, {'kind': 'key', 'key': 'F'}]})
wl = run('bone.world', {'bone': 'wrist.L'})['position']
tgt = cam(r)['target']
check('F frames the hand close up', cam(r)['distance'] < 1.5 and sum((a - b) ** 2 for a, b in zip(tgt, wl)) ** 0.5 < 0.15, cam(r))
shot('live-framed-hand')
req({'op': 'ui', 'name': 'select', 'value': 'none'})
r = req({'op': 'input', 'events': [HOVER, {'kind': 'key', 'key': 'F'}]})
check('F with nothing selected frames the body', cam(r)['distance'] > 2.0, cam(r))

# follow: the camera keeps the root in the middle
req({'op': 'ui', 'name': 'follow', 'value': 'on'})
run('frame.set', {'frame': 60})
r = req({'op': 'input', 'events': [HOVER]})
root = run('bone.world', {'bone': 'root'})['position']
check('follow keeps the root in the middle', abs(cam(r)['target'][0] - root[0]) < 1e-4 and abs(cam(r)['target'][1] - root[1]) < 1e-4, (cam(r)['target'], root))
req({'op': 'ui', 'name': 'follow', 'value': 'off'})
run('frame.set', {'frame': 0})

# shift+click adds joints to the selection (Drag tool)
req({'op': 'ui', 'name': 'tool', 'value': 'drag'})
req({'op': 'ui', 'name': 'camera', 'value': 'front'})
lay = req({'op': 'input', 'events': [HOVER]})['layout']
(ax, ay), (bx, by) = lay['joints']['wrist.L'], lay['joints']['wrist.R']
req({'op': 'input', 'events': [{'kind': 'click', 'x': ax, 'y': ay}]})
r = req({'op': 'input', 'events': [{'kind': 'click', 'x': bx, 'y': by, 'modifiers': ['shift']}]})
sel = r['layout']['selection']
check('shift+click adds a joint', sorted(sel['joints']) == ['wrist.L', 'wrist.R'] and sel['active'] == 'wrist.R', sel)

# finger and face handles
req({'op': 'ui', 'name': 'handles', 'value': 'all'})
lay = req({'op': 'input', 'events': [HOVER]})['layout']
check('finger and face handles show', 'finger2-1.L' in lay['joints'] and 'eye.R' in lay['joints'], len(lay['joints']))
req({'op': 'ui', 'name': 'handles', 'value': 'body'})

# the picker: click joints on the figure, a part, mirror
req({'op': 'ui', 'name': 'picker', 'value': 'on'})
lay = req({'op': 'input', 'events': [HOVER]})['layout']
pk = lay['picker']
px, py = pk['joints']['wrist.L']
r = req({'op': 'input', 'events': [{'kind': 'click', 'x': px, 'y': py}]})
check('picker click selects a joint', r['layout']['selection'] == {'active': 'wrist.L', 'joints': ['wrist.L']}, r['layout']['selection'])
qx, qy = pk['joints']['upperleg01.L']
r = req({'op': 'input', 'events': [{'kind': 'click', 'x': qx, 'y': qy, 'modifiers': ['shift']}]})
check('picker shift+click adds a joint', sorted(r['layout']['selection']['joints']) == ['upperleg01.L', 'wrist.L'], r['layout']['selection'])
mx, my = pk['buttons']['Mirror']
r = req({'op': 'input', 'events': [{'kind': 'click', 'x': mx, 'y': my}]})
sel = r['layout']['selection']
check('Mirror selects the other side', sorted(sel['joints']) == ['upperleg01.R', 'wrist.R'] and sel['active'] == 'upperleg01.R', sel)
lx, ly = pk['buttons']['Left arm']
r = req({'op': 'input', 'events': [{'kind': 'click', 'x': lx, 'y': ly}]})
sel = r['layout']['selection']
check('a part button selects the part', 'clavicle.L' in sel['joints'] and 'wrist.L' in sel['joints'] and 'wrist.R' not in sel['joints'], sel['active'])
shot('live-picker')
req({'op': 'ui', 'name': 'picker', 'value': 'off'})
# the agent protocol: replies echo the request's id; events since a cursor; a subscription on a
# second connection hears commands and undo as they happen; the animation op makes a GIF
f.write(json.dumps({'op': 'events', 'since': 0, 'limit': 1000, 'id': 'ev1'}) + '\n'); f.flush()
r = json.loads(f.readline())
check('replies echo the request id', r.get('id') == 'ev1', r.get('id'))
evs = r['ok']['events']
check('gestures are events from the ui', any(e.get('source') == 'ui' and e['event'] == 'command' for e in evs),
      [e for e in evs if e['event'] == 'command'][:3])
check('selection and playhead changes are events', {'selection', 'frame', 'undo'} <= {e['event'] for e in evs},
      sorted({e['event'] for e in evs}))
cursor = r['ok']['latest']
sub = socket.create_connection(('127.0.0.1', PORT))
sub.settimeout(10)
sf = sub.makefile('rw', encoding='utf-8')
sf.write(json.dumps({'op': 'subscribe', 'events': ['command', 'undo']}) + '\n'); sf.flush()
check('subscribe answers with the cursor', json.loads(sf.readline())['ok'].get('cursor') == cursor)
run('key.set')
run('edit.undo')
got = [json.loads(sf.readline()) for _ in range(3)]
check('the subscriber hears the commands and the undo',
      [(g['event'], g.get('id')) for g in got] == [('command', 'key.set'), ('command', 'edit.undo'), ('undo', None)], got)
sub.close()
later = req({'op': 'events', 'since': cursor})
check('events since the cursor are the new ones', [e['seq'] for e in later['events']][:1] == [cursor + 1], later['cursor'])
a = req({'op': 'animation', 'times': [0, 10, 20], 'width': 64, 'height': 48, 'view': 'side'})
check('the animation op makes a GIF', a['frames'] == 3 and base64.b64decode(a['gif'])[:6] == b'GIF89a', a['frames'])
# ---- the timeline's menus, keyboard, copy / paste, dope rows, retime, markers, play range,
# problem marks, curves and the foot-contact strip ----
def settle(n=2, at=(700, 400)):
    return req({'op': 'input', 'events': [{'kind': 'move', 'x': at[0], 'y': at[1]}] * n})


def click(px, py, button='primary', mods=()):
    return req({'op': 'input', 'events': [{'kind': 'click', 'x': px, 'y': py, 'button': button, 'modifiers': list(mods)}]})


def key(name, mods=()):
    return req({'op': 'input', 'events': [{'kind': 'key', 'key': name, 'modifiers': list(mods)}]})


def menu_pick(path, at):
    """Right-click at `at`, then click the menu items named in `path` (submenus hovered first)."""
    click(*at, button='secondary')
    settle(2, (at[0] + 2, at[1]))
    for i, name in enumerate(path):
        items = req({'op': 'layout'}).get('menu', {})
        if name not in items:
            return f'no menu item {name!r} in {sorted(items)}'
        mx, my = items[name]
        if i < len(path) - 1:
            settle(4, (mx, my))
        else:
            click(mx, my)
    return None


def timeline():
    global tl, fx, ky
    tl = req({'op': 'layout'})['timeline']
    fx, ky = tl['frame_x'], tl['keys_y']
    return tl


def last_undo():
    u = run('edit.undo')
    return u.get('label')


req({'op': 'ui', 'name': 'tool', 'value': 'drag'})
req({'op': 'ui', 'name': 'fit', 'value': '1'})
run('frame.set', {'frame': 0})
settle()
timeline()
check('timeline batch starts on 0 25 40 60', keys() == [0, 25, 40, 60], keys())

# right-click a key, Interpolation > Step: the key's glyph and its interpolation change
err = menu_pick(['interp', 'interp:step'], (x(25), ky))
k25 = next(k for k in run('editor.state')['keys'] if k['frame'] == 25)
check('key menu sets the interpolation', err is None and k25['interp'] == 'step', err or k25)
check('interpolation is one undo step', last_undo() == 'Step (hold) on 1 key')

# keyboard: a click gives the timeline the keyboard; arrows, Tab, Shift+Tab, Home, End
r = click(x(10), ky)
check('a click focuses the timeline', req({'op': 'layout'})['timeline']['focused'] is True)
nav = []
for k, m in [('ArrowRight', ()), ('Tab', ()), ('Tab', ('shift',)), ('End', ()), ('Home', ())]:
    nav.append(key(k, m)['frame'])
check('arrows / Tab / Home / End walk the keys', nav == [25, 40, 25, 60, 0], nav)

# copy two keys, paste at 70 (one undo), paste mirrored
drag(x(20), ky - 4, x(45), ky + 4)
settle(2, (x(50), ky))
key('C', ('ctrl',))
run('frame.set', {'frame': 70})
key('V', ('ctrl',))
check('Ctrl+V pastes the copied keys at the playhead', keys() == [0, 25, 40, 60, 70, 85], keys())
check('paste is one undo step', last_undo() == 'Paste 2 keys at 70')
key('V', ('ctrl', 'shift'))
check('Ctrl+Shift+V pastes them mirrored', keys() == [0, 25, 40, 60, 70, 85], keys())
check('mirrored paste names itself', last_undo() == 'Paste 2 keys mirrored at 70')

# right-click empty space at 10, Retime > Insert 5 frames: keys from 10 on slide 5 later
run('frame.set', {'frame': 0})
err = menu_pick(['retime', 'insert'], (x(10), ky))
check('retime menu inserts frames', err is None and keys() == [0, 30, 45, 65], err or keys())
check('retime is one undo step', last_undo() == 'Retime keys')

# dope sheet: drag the Left arm row's key 40 to 45; Delete on the Head row's key 60
req({'op': 'ui', 'name': 'dope', 'value': 'on'})
settle()
rows = timeline()['rows_y']
drag(x(40), rows['Left arm'], x(45), rows['Left arm'])
st = {k['frame']: k for k in run('editor.state')['keys']}
check('a row drag moves that part only', sorted(st) == [0, 25, 40, 45, 60] and st[45].get('part') == 'Left arm' and 'part' in st[40], sorted(st))
check('row drag is one undo step', last_undo() == 'Move Left arm on 1 key by 5')
r = click(x(60), rows['Head'])
check('a row click selects that part of the key', timeline()['part'] == 'Head' and r['selected_keys'] == [60], (tl['part'], r['selected_keys']))
settle(1, (700, 300))
key('Delete')
k60 = next(k for k in run('editor.state')['keys'] if k['frame'] == 60)
check('Delete on a row removes that part', 'Head' not in k60.get('rows', ['Head']), k60)
check('removing a part is one undo step', last_undo() == 'Remove Head from 1 key')
shot('live-dope')
req({'op': 'ui', 'name': 'dope', 'value': 'off'})
settle()
timeline()

# markers: add from the ruler's menu, drag it, click jumps there, double-click renames
err = menu_pick(['add_marker'], (x(30), tl['ruler_y']))
check('ruler menu adds a marker', err is None and run('editor.state')['markers'] == [{'frame': 30, 'name': 'Marker 1'}], err)
my = timeline()['markers_y']
drag(x(30) + 2, my, x(35) + 2, my)
check('dragging a marker moves it', run('editor.state')['markers'] == [{'frame': 35, 'name': 'Marker 1'}], run('editor.state')['markers'])
run('frame.set', {'frame': 0})
r = click(x(35) + 2, my)
check('clicking a marker goes there', r['frame'] == 35, r['frame'])
# wait out egui's double-click time after the click above, or the pair counts as clicks 2 and 3
time.sleep(0.5)
req({'op': 'input', 'events': [{'kind': 'click', 'x': x(35) + 2, 'y': my}] * 2})
check('double-clicking a marker opens its rename box', timeline()['renaming'] == 35, tl['renaming'])
key('Escape')
check('Escape closes the rename box', timeline()['renaming'] is None, tl['renaming'])

# play range: drag the out handle from the end to 50; I sets the in point at the playhead
drag(x(60.5), my, x(50), my)
check('dragging the out handle sets the play range', timeline()['range'] == [0, 50], tl['range'])
run('frame.set', {'frame': 10})
settle(1, (x(20), ky))
key('I')
check('I sets the in point at the playhead', timeline()['range'] == [10, 50], tl['range'])
req({'op': 'ui', 'name': 'range', 'value': 'off'})

# problem marks from another panel: drawn on their lane, a click jumps there
req({'op': 'ui', 'name': 'marks', 'value': '12:Left foot slides 4 cm;50-56:Off balance'})
settle()
check('marks are on the timeline', [m['frame'] for m in timeline()['marks']] == [12, 50], tl['marks'])
r = click(x(12), tl['marks_y'])
check('clicking a mark goes there', r['frame'] == 12, r['frame'])
shot('live-markers-marks')
req({'op': 'ui', 'name': 'marks', 'value': 'off'})

# curves: wrist.R's Z angle key on 40 dragged up (Shift: value only) runs key.channel
req({'op': 'ui', 'name': 'curves', 'value': 'on'})
req({'op': 'ui', 'name': 'joint', 'value': 'wrist.R'})
settle()
cl = req({'op': 'layout'})['curves']
dot = next((d for d in cl['keys'] if d['frame'] == 40 and d['channel'] == 2), None)
z0 = next(k['values'][2] for k in run('clip.curves', {'bone': 'wrist.R'})['keys'] if k['frame'] == 40)
if dot:
    drag(dot['x'], dot['y'], dot['x'], dot['y'] - 40, mods=('shift',))
z1 = next(k['values'][2] for k in run('clip.curves', {'bone': 'wrist.R'})['keys'] if k['frame'] == 40)
# 40 points up on a graph whose values lo..hi span its height less 12 points of margin; the X,
# Y and Z dots all sit on 0, and the drag takes the Z one, drawn on top
lo, hi = cl['values']
want = z0 + 40 * (hi - lo) / (cl['rect'][3] - cl['rect'][1] - 12)
check('dragging a curve key sets its value', dot is not None and abs(z1 - want) < 0.01, f'{z0:.3f} -> {z1:.3f}, want {want:.3f}')
shot('live-curves')
check('a curve key drag is one undo step', last_undo() == 'Set key channel')
req({'op': 'ui', 'name': 'curves', 'value': 'off'})

# the contact strip, on an animation whose feet stand: a click pins the left foot's span,
# another frees it
run('clip.new', {'name': 'Contacts'})
run('pose.set_root', {'root': [0, 0, 0.9450101]})
run('key.set', {'interp': 'linear'})
run('frame.set', {'frame': 20})
run('pose.set_root', {'root': [0, 0.02, 0.9450101]})
run('key.set', {'interp': 'linear'})
req({'op': 'ui', 'name': 'contacts', 'value': 'on'})
req({'op': 'ui', 'name': 'fit', 'value': '1'})
settle()
cy = timeline()['contacts_y']
click(x(10), cy['L'])
pinned = [k['frame'] for k in run('editor.state')['keys'] if k.get('model') == 'foot-lock']
check('clicking a planted span pins the foot', pinned == list(range(1, 20)), pinned)
shot('live-contacts')
click(x(10), cy['L'])
pinned = [k['frame'] for k in run('editor.state')['keys'] if k.get('model') == 'foot-lock']
check('clicking it again frees the foot', pinned == [], pinned)
req({'op': 'ui', 'name': 'contacts', 'value': 'off'})

# ---- one key, one action, by where it is: Ctrl+C copies the selected keys over the timeline and
# the pose over the 3D view; the arrows walk keys only while the timeline has the keyboard; F
# frames only over the 3D view; a key runs once ----
def commands_after(c):
    return [e.get('id') for e in req({'op': 'events', 'since': c})['events'] if e['event'] == 'command']


def latest():
    return req({'op': 'events', 'since': 0, 'limit': 1})['latest']


vp = req({'op': 'layout'})['viewport']
EMPTY = (vp[0] + 30, vp[3] - 30)  # bare floor in the 3D view, away from the body and its toolbar
run('frame.set', {'frame': 10})
timeline()
click(x(20), ky)
click(*EMPTY)
check('a click in the 3D view takes the keyboard from the timeline',
      timeline()['focused'] is False and timeline()['selected'] == [20], (tl['focused'], tl.get('selected')))
r = key('ArrowLeft')
check('Left steps a frame when the timeline has not the keyboard', r['frame'] == 19, r['frame'])
c = latest()
key('C', ('ctrl',))
got = commands_after(c)
check('Ctrl+C over the 3D view copies the pose, not the keys', 'pose.copy' in got and 'keys.copy' not in got, got)
settle(2, (x(5), ky))
c = latest()
key('C', ('ctrl',))
got = commands_after(c)
check('Ctrl+C over the timeline copies the selected keys', got == ['keys.copy'], got)
before = req({'op': 'layout'})['camera']
r = key('F')
check('F over the timeline leaves the camera', r['layout']['camera'] == before, (before, r['layout']['camera']))
run('frame.set', {'frame': 10})
c = latest()
key('A', ('shift',))
got = commands_after(c)
check('Shift+A runs once', got.count('ai.toggle') == 1, got)

# ---- stable widget ids: op layout lists them; op click presses one by id ----
ids = {w['id']: w for w in req({'op': 'layout'})['widgets']}
check('layout lists widgets with ids, labels and rects',
      ids.get('timeline.dope', {}).get('label') == 'Dope sheet' and ids['timeline.dope']['rect'] and 'toolbar.tool.rotate' in ids
      and 'library.tab.animations' in ids and 'panel.layers' in ids and 'menu.Edit.Undo' in ids, len(ids))
r = req({'op': 'click', 'widget': 'toolbar.tool.rotate'})
check('click presses a toolbar button', r.get('clicked') == 'toolbar.tool.rotate' and r['layout'].get('tool') == 'rotate', r['layout'].get('tool'))
req({'op': 'click', 'widget': 'toolbar.tool.drag'})
r = req({'op': 'click', 'widget': 'timeline.dope'})
check('click toggles the dope sheet', 'Left arm' in (timeline().get('rows_y') or {}), tl.get('rows_y'))
req({'op': 'click', 'widget': 'timeline.dope'})
settle()
req({'op': 'input', 'events': [HOVER, {'kind': 'key', 'key': '0'}]})
r = req({'op': 'click', 'widget': 'toolbar.view.front'})
check('click opens the View menu, then presses Front', abs(r['layout']['camera']['yaw']) < 1e-6, r['layout']['camera'])
n = len(keys())
run('frame.set', {'frame': 50})
run('key.set')
r = req({'op': 'click', 'widget': 'menu.Edit.Undo'})
check('click runs a menu bar item', r.get('invoked') == 'edit.undo' and len(keys()) == n, (r.get('invoked'), keys()))
f.write(json.dumps({'op': 'click', 'widget': 'timeline.dpe'}) + '\n'); f.flush()
e = json.loads(f.readline()).get('error', '')
check('an unknown id is refused with the nearest ids', 'timeline.dope' in e, e)

# footsteps: two standing keys 1.3 m apart; the gap's menu, Footsteps > Right foot first, walks it
# with the built-in Walk loop (a key every 2 frames and around each footfall), as one undo
run('clip.new', {'name': 'Footsteps'})
run('pose.set_root', {'root': [0, 0, 0.9450101]})
run('key.set', {'interp': 'smooth'})
run('frame.set', {'frame': 40})
run('pose.set_root', {'root': [0, -1.5, 0.9450101]})
run('key.set', {'interp': 'smooth'})
req({'op': 'ui', 'name': 'fit', 'value': '1'})
settle()
timeline()
settle(2, (x(20), ky))
err = menu_pick(['steps', 'steps:R'], (x(20), ky))
st = run('editor.state')
mine = [k['frame'] for k in st['keys'] if k['source'] == 'manual']
check('Footsteps walks with keys on each footfall', err is None and {0, 13, 27, 40} <= set(mine) and len(mine) > 20, err or mine)
check('the Walk loop needs no model', not st['bands'], st['bands'])
shot('live-footsteps')
check('footsteps are one undo step', last_undo() == 'Footsteps')

failed = [n for n, ok in checks if not ok]
print(f'{len(checks)} checks, {len(failed)} failed')
s.close()
app.kill()
sys.exit(1 if failed else 0)

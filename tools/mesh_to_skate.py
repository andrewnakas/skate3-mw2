"""Write a .skate map (SKATE09, zlib blocks) from a plain mesh description.

Used by the clean-room MW2 map pipeline (skate3-cleanroom games/mw2) and by
anyone building an original park from their own geometry. Everything is
Y-up metres, the engine's own space.

Input directory:

  map.json
    {"name": "Rust (clean room)",
     "spawn": [x, y, z], "heading": 0.0,
     "environment": [45 floats]            # optional; defaults to a clear day
     "materials": [{"name": "...", "texture": "textures/x.png" | null,
                    "color": [r, g, b], "roughness": 0.9,
                    "friction": 0.6, "restitution": 0.1,
                    "alpha_mode": 0, "alpha_cutoff": 0.5,      # 0 opaque, 1 mask, 2 blend
                    "audio": 3, "physics": 1, "pattern": 0}],  # packed RW surface id parts
     "lights": [{"name", "kind", "position", "direction", "color",
                 "intensity", "range", "radius", "inner_cos", "outer_cos"}],
     "rails": [{"name", "closed", "points": [[x, y, z], ...]}]}

  render.npz     positions (N,3) f32, normals (N,3) f32, uv (N,2) f32,
                 material (N,) u32 (1-based), indices (M,) u32;
                 optional lightmap_uv (N,2) f32
  collision.npz  triangles (K,3,3) f32, material (K,) u32 (1-based);
                 optional surface (K,) u32 (tag, defaults to the material id)

  python tools/mesh_to_skate.py <input dir> <out.skate>
  python tools/mesh_to_skate.py --demo <dir>     # writes a tiny example input
"""
import json
import pathlib
import struct
import sys
import zlib

import numpy as np

VERSION = 9
# The demo map's clear day (tools/make_skate_demo.py), 45 floats.
DEFAULT_ENVIRONMENT = [
    .09, .34, .72, .58, .78, .98, .18, .25, .34, 0, 12, .62, 17, 0,
    .045, .10, .26, 1, .32, .10, .05, .035, .06,
    .007, .015, .045, .045, .085, .17, .008, .014, .032,
    1, .92, .78, .42, .56, .92, 1.25, .18, .32, .11, 1, 1, 1,
]


class Out:
    def __init__(self):
        self.b = bytearray()

    def u(self, *v):
        self.b += struct.pack("<" + "I" * len(v), *[int(x) for x in v])

    def f(self, *v):
        self.b += struct.pack("<" + "f" * len(v), *[float(x) for x in v])

    def s(self, text):
        data = text.encode("utf-8")
        self.u(len(data))
        self.b += data

    def stored(self, raw):
        """A v9 stored block: method 1 (zlib) when it helps, else 0 (raw)."""
        packed = zlib.compress(bytes(raw), 9)
        if len(packed) < len(raw):
            self.u(1, len(packed))
            self.b += packed
        else:
            self.u(0, len(raw))
            self.b += raw


def load_rgba(path):
    from PIL import Image
    img = Image.open(path).convert("RGBA")
    return img.width, img.height, img.tobytes()


def write(src, dst):
    src = pathlib.Path(src)
    spec = json.loads((src / "map.json").read_text())
    render = np.load(src / "render.npz")
    collision = np.load(src / "collision.npz")

    positions = np.asarray(render["positions"], np.float32).reshape(-1, 3)
    normals = np.asarray(render["normals"], np.float32).reshape(-1, 3)
    uv = np.asarray(render["uv"], np.float32).reshape(-1, 2)
    lm = np.asarray(render["lightmap_uv"], np.float32).reshape(-1, 2) if "lightmap_uv" in render else np.zeros_like(uv)
    vmat = np.asarray(render["material"], np.uint32).reshape(-1)
    indices = np.asarray(render["indices"], np.uint32).reshape(-1)
    tris = np.asarray(collision["triangles"], np.float32).reshape(-1, 3, 3)
    cmat = np.asarray(collision["material"], np.uint32).reshape(-1)
    surf = np.asarray(collision["surface"], np.uint32).reshape(-1) if "surface" in collision else cmat

    materials = spec["materials"]
    n = len(positions)
    assert len(normals) == n and len(uv) == n and len(lm) == n and len(vmat) == n, "vertex arrays differ in length"
    assert len(indices) % 3 == 0 and indices.max(initial=0) < n, "bad indices"
    assert len(cmat) == len(tris) == len(surf), "collision arrays differ in length"
    for ids in (vmat, cmat):
        assert ids.min(initial=1) >= 1 and ids.max(initial=1) <= len(materials), "material ids are 1-based"
    for name, a in (("positions", positions), ("normals", normals), ("uv", uv), ("collision", tris)):
        assert np.isfinite(a).all(), f"{name} has non-finite values"

    # Textures: one per material that names a file, deduplicated by path.
    textures, tex_ids = [], {}
    for m in materials:
        path = m.get("texture")
        if path and path not in tex_ids:
            textures.append(path)
            tex_ids[path] = len(textures)

    o = Out()
    o.b += b"SKATE%02d\0" % VERSION
    o.u(0x12345678)
    o.s(spec["name"])
    o.f(*spec.get("spawn", [0, 0, 0]))
    o.f(spec.get("heading", 0.0))
    env = spec.get("environment", DEFAULT_ENVIRONMENT)
    assert len(env) == 45, "environment needs 45 floats"
    o.f(*env)
    rails = spec.get("rails", [])
    lights = spec.get("lights", [])
    o.u(len(materials), len(textures), n, len(indices), len(tris), len(rails), 0, len(lights), 0)

    for m in materials:
        o.s(m["name"])
        o.u(m.get("flags", 1))
        o.f(m.get("friction", 0.6), m.get("restitution", 0.1))
        o.f(*m.get("color", [1, 1, 1]))
        o.f(m.get("roughness", 0.9), m.get("emissive", 0.0))
        o.u(tex_ids.get(m.get("texture"), 0), 0)       # albedo, indirect lightmap
        o.f(m.get("indirect_strength", 1.0))
        o.u(0, 0, 0)                                    # normal, ORM, emissive maps
        o.u(m.get("alpha_mode", 0))
        o.f(m.get("alpha_cutoff", 0.5))
        o.u(m.get("audio", 3), m.get("physics", 1), m.get("pattern", 0))

    for path in textures:
        w, h, rgba = load_rgba(src / path)
        o.s(pathlib.Path(path).stem)
        o.u(w, h, 1)                                    # colour space 1: sRGB albedo
        o.stored(rgba)

    vertex = np.zeros(n, dtype=[("p", "<f4", 3), ("n", "<f4", 3), ("uv", "<f4", 2), ("lm", "<f4", 2), ("m", "<u4")])
    vertex["p"], vertex["n"], vertex["uv"], vertex["lm"], vertex["m"] = positions, normals, uv, lm, vmat
    assert vertex.dtype.itemsize == 44
    o.stored(vertex.tobytes())
    o.stored(indices.astype("<u4").tobytes())
    coll = np.zeros(len(tris), dtype=[("p", "<f4", 9), ("s", "<u4"), ("m", "<u4")])
    coll["p"], coll["s"], coll["m"] = tris.reshape(-1, 9), surf, cmat
    assert coll.dtype.itemsize == 44
    o.stored(coll.tobytes())

    for r in rails:
        o.s(r["name"])
        o.u(1 if r.get("closed") else 0)
        o.u(len(r["points"]))
        for p in r["points"]:
            o.f(*p)
    for l in lights:
        o.s(l["name"])
        o.u(l.get("kind", 0))
        o.f(*l["position"])
        o.f(*l.get("direction", [0, -1, 0]))
        o.f(*l.get("color", [1, 1, 1]))
        o.f(l.get("intensity", 1000.0), l.get("range", 10.0), l.get("radius", 0.1),
            l.get("inner_cos", 1.0), l.get("outer_cos", 0.0))

    pathlib.Path(dst).write_bytes(o.b)
    print(f"Wrote {dst}: {len(materials)} materials, {len(textures)} textures, "
          f"{n} vertices, {len(indices) // 3} triangles, {len(tris)} collision, {len(o.b)} bytes")


def demo(dst):
    """A 40 m square with a box, an original example input for the writer."""
    from PIL import Image
    dst = pathlib.Path(dst)
    (dst / "textures").mkdir(parents=True, exist_ok=True)
    checker = np.indices((64, 64)).sum(axis=0) // 8 % 2
    rgb = np.where(checker[..., None] == 1, [196, 170, 120], [150, 128, 90]).astype(np.uint8)
    Image.fromarray(rgb, "RGB").save(dst / "textures" / "sand.png")
    quads = [
        [(-20, 0, -20), (-20, 0, 20), (20, 0, 20), (20, 0, -20)],
        [(-2, 1, -2), (-2, 1, 2), (2, 1, 2), (2, 1, -2)],
    ]
    pos, nrm, uv, idx, coll = [], [], [], [], []
    for q in quads:
        base = len(pos)
        for p in q:
            pos.append(p)
            nrm.append((0, 1, 0))
            uv.append((p[0] / 4, p[2] / 4))
        idx += [base, base + 1, base + 2, base, base + 2, base + 3]
        coll += [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]
    np.savez(dst / "render.npz", positions=np.float32(pos), normals=np.float32(nrm), uv=np.float32(uv),
             material=np.ones(len(pos), np.uint32), indices=np.uint32(idx))
    np.savez(dst / "collision.npz", triangles=np.float32(coll), material=np.ones(len(coll), np.uint32))
    (dst / "map.json").write_text(json.dumps({
        "name": "Mesh writer demo",
        "spawn": [0, 0, 6], "heading": 0,
        "materials": [{"name": "Sand", "texture": "textures/sand.png"}],
        "rails": [{"name": "Box edge", "points": [[-2, 1, 2], [2, 1, 2]]}],
    }, indent=1))
    print(f"Wrote demo input to {dst}")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--demo":
        demo(sys.argv[2])
    elif len(sys.argv) == 3:
        write(sys.argv[1], sys.argv[2])
    else:
        sys.exit(__doc__)

#!/usr/bin/env python3
"""Generate a tagged, first-order tetrahedral mesh from a bridge FEA bundle.

Gmsh owns volume meshing; descriptors match faces without relying on numbering.
Failures and ambiguous matches publish no output. Element limits apply after
meshing, not to Gmsh's internal workspace. Dependencies: gmsh 4.15.2.
"""
import argparse
from collections import defaultdict
import itertools
import json
import math
import os
from pathlib import Path
import tempfile


def finite_vector(value, length):
    return (isinstance(value, list) and len(value) == length
            and all(isinstance(item, (int, float)) and not isinstance(item, bool)
                    and math.isfinite(item) for item in value))


def load_manifest(path):
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError("manifest must be an object")
    if data.get("version") != 1 or data.get("units") != "mm":
        raise ValueError("unsupported volume manifest version or units")
    names = data.get("face_tags")
    if not isinstance(names, list) or not all(isinstance(name, str) for name in names) or len(set(names)) != len(names):
        raise ValueError("physical names must be unique")
    for name in [data.get("volume_name"), *names]:
        if (not isinstance(name, str) or not name.strip() or len(name.encode()) > 128
                or any(ord(character) < 32 or character in '\\"' for character in name)):
            raise ValueError("invalid physical name")
    if len(set([data["volume_name"], *names, "__unassigned_boundary"])) != len(names) + 2:
        raise ValueError("volume, boundary, and reserved physical names must be distinct")
    faces = data.get("faces")
    if not isinstance(faces, list) or not faces:
        raise ValueError("manifest requires face descriptors")
    ids = set()
    for face in faces:
        if (not isinstance(face, dict) or not isinstance(face.get("index"), int)
                or isinstance(face["index"], bool) or face["index"] < 0
                or face["index"] in ids):
            raise ValueError("face indices must be unique nonnegative integers")
        ids.add(face["index"])
        tag = face.get("physical_tag")
        if not isinstance(tag, int) or isinstance(tag, bool) or not 0 <= tag <= len(names):
            raise ValueError("invalid face physical tag")
        area = face.get("area_mm2")
        if not isinstance(area, (int, float)) or isinstance(area, bool) or not math.isfinite(area) or area <= 0:
            raise ValueError("face area must be finite and positive")
        if (not finite_vector(face.get("center_mm"), 3)
                or not isinstance(face.get("bounds_mm"), list) or len(face["bounds_mm"]) != 2
                or not all(finite_vector(bound, 3) for bound in face["bounds_mm"])
                or any(a > b for a, b in zip(*face["bounds_mm"]))):
            raise ValueError("invalid face center or bounds")
    return data


def match_faces(expected, actual, tolerance):
    """Spatial/area buckets avoid an all-face scan for each imported face.

    Pathological coincident descriptors remain ambiguous and fail explicitly.
    """
    if len(expected) != len(actual):
        raise ValueError("BREP face count differs from the manifest")
    area_tolerance = max(1e-8, max(face["area_mm2"] for face in expected) * 1e-7)

    def key(face):
        return (*[math.floor(value / tolerance) for value in face["center_mm"]],
                math.floor(face["area_mm2"] / area_tolerance))

    buckets = defaultdict(list)
    for face in expected:
        buckets[key(face)].append(face)
    result, used = {}, set()
    offsets = list(itertools.product((-1, 0, 1), repeat=4))
    for imported in actual:
        center = key(imported)
        matches = []
        for offset in offsets:
            for face in buckets.get(tuple(a + b for a, b in zip(center, offset)), []):
                if (abs(face["area_mm2"] - imported["area_mm2"]) <= area_tolerance
                        and all(abs(a - b) <= tolerance for a, b in zip(face["center_mm"], imported["center_mm"]))
                        and all(abs(a - b) <= tolerance for left, right in zip(face["bounds_mm"], imported["bounds_mm"])
                                for a, b in zip(left, right))):
                    matches.append(face)
                    if len(matches) > 1:
                        raise ValueError("ambiguous BREP face descriptors; refusing to assign a boundary tag")
        if len(matches) != 1 or matches[0]["index"] in used:
            raise ValueError("imported BREP face does not uniquely match its descriptor")
        used.add(matches[0]["index"])
        result[imported["entity"]] = matches[0]["physical_tag"]
    return result


def generate(bundle, output, size, maximum_elements, face_tolerance):
    manifest = load_manifest(bundle / "volume.json")
    if (not math.isfinite(size) or size <= 0 or not 1 <= maximum_elements <= 1_000_000
            or not math.isfinite(face_tolerance) or face_tolerance <= 0):
        raise ValueError("invalid mesh size, element budget, or face tolerance")
    if output.exists():
        raise FileExistsError("output already exists")
    import gmsh  # Optional external dependency, loaded only for actual meshing.
    gmsh.initialize()
    temporary = None
    try:
        gmsh.option.setNumber("General.NumThreads", 1)
        gmsh.option.setNumber("General.Verbosity", 2)
        gmsh.model.add("bridge-volume")
        gmsh.model.occ.importShapes(str(bundle / "body.brep"), highestDimOnly=True)
        gmsh.model.occ.synchronize()
        volumes = gmsh.model.getEntities(3)
        if len(volumes) != 1:
            raise ValueError("volume meshing requires exactly one imported solid")
        volume = gmsh.model.occ.getMass(*volumes[0])
        if not math.isfinite(volume) or volume <= 0 or math.log(volume) - 3*math.log(size) + math.log(6) > math.log(maximum_elements):
            raise ValueError("requested size is below the element-budget estimate")
        actual = []
        for dimension, entity in gmsh.model.getEntities(2):
            bounds = gmsh.model.occ.getBoundingBox(dimension, entity)
            actual.append({"entity": entity, "area_mm2": gmsh.model.occ.getMass(dimension, entity),
                           "center_mm": list(gmsh.model.occ.getCenterOfMass(dimension, entity)),
                           "bounds_mm": [list(bounds[:3]), list(bounds[3:])]})
        matched = match_faces(manifest["faces"], actual, face_tolerance)
        groups = defaultdict(list)
        for entity, tag in matched.items():
            groups[tag].append(entity)
        for tag, entities in sorted(groups.items()):
            physical = tag or len(manifest["face_tags"]) + 1
            name = manifest["face_tags"][tag - 1] if tag else "__unassigned_boundary"
            gmsh.model.addPhysicalGroup(2, entities, physical)
            gmsh.model.setPhysicalName(2, physical, name)
        gmsh.model.addPhysicalGroup(3, [volumes[0][1]], 1)
        gmsh.model.setPhysicalName(3, 1, manifest["volume_name"])
        gmsh.option.setNumber("Mesh.MeshSizeMin", size * 1e-3)
        gmsh.option.setNumber("Mesh.MeshSizeMax", size)
        gmsh.option.setNumber("Mesh.MeshSizeFromCurvature", 24)
        gmsh.option.setNumber("Mesh.ElementOrder", 1)
        gmsh.option.setNumber("Mesh.MshFileVersion", 4.1)
        gmsh.option.setNumber("Mesh.Binary", 0)
        gmsh.model.mesh.generate(3)
        types, tags, _ = gmsh.model.mesh.getElements(3)
        if len(types) != 1 or int(types[0]) != 4 or not len(tags[0]):
            raise ValueError("mesher did not produce first-order tetrahedra")
        count = len(tags[0])
        if count > maximum_elements:
            raise ValueError("generated tetrahedra exceed the element budget")
        qualities = gmsh.model.mesh.getElementQualities(tags[0], "minSICN")
        if any(not math.isfinite(value) or value <= 0 for value in qualities):
            raise ValueError("volume mesh contains invalid or inverted tetrahedra")
        descriptor, filename = tempfile.mkstemp(prefix=".occt-volume-", suffix=".msh", dir=output.parent)
        os.close(descriptor)
        temporary = Path(filename)
        gmsh.write(str(temporary))
        with temporary.open("rb") as stream:
            os.fsync(stream.fileno())
        os.link(temporary, output)  # Atomic publication; fails if output appeared.
        return {"tetrahedra": count, "minimum_inverse_condition": float(min(qualities)),
                "matched_faces": len(matched), "units": "mm"}
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
        gmsh.finalize()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--size-mm", type=float, required=True)
    parser.add_argument("--maximum-elements", type=int, default=1_000_000)
    parser.add_argument("--face-tolerance-mm", type=float, default=1e-5)
    arguments = parser.parse_args()
    try:
        result = generate(arguments.bundle, arguments.output, arguments.size_mm,
                          arguments.maximum_elements, arguments.face_tolerance_mm)
    except Exception as error:  # Gmsh also raises its own generic API exceptions.
        parser.exit(1, f"volume mesh: {error}\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()

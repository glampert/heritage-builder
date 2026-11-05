"""Heritage Builder tile generator.

The package splits in two:
- Blender modules (import bpy): palettes, materials, builder, scene, recipes,
  generate. Import them only inside Blender.
- Plain-Python modules (standard library only): catalog, pngio, tileset. The
  tools that run outside Blender (sync_tileset.py, validate.py, CI) use these.

Importing the package itself imports nothing, so plain Python can use the
second group.
"""

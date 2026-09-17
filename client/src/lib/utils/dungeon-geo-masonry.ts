import * as THREE from 'three'
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js'
import type { GeoEntry } from './house-geo-utils'
import { HOUSING_TEXTURES } from './housing-textures'
import { DUNGEON_FLOOR_UV_SCALE, WALL_THICKNESS } from './dungeon-geo-constants'
import { quadMeshBuilder } from './dungeon-geo-primitives'

const BRICK_WIDTH = 4 / 9
const BRICK_HEIGHT = 2 / 9
const TILE_SIZE = 1 / (4 * DUNGEON_FLOOR_UV_SCALE)
const TILES_PER_CELL = Math.round(1 / TILE_SIZE)
const TILE_DEPTH = 0.045
const SOIL_TEXTURE = HOUSING_TEXTURES.findIndex(
  (entry) => entry.glb === 'red_laterite_soil_stones_1k'
)
const BRICK_FACES = [
  [0.18, 0.06, 0.54, 0.23],
  [0.36, 0.28, 0.8, 0.43],
  [0.17, 0.5, 0.54, 0.64],
  [0.35, 0.71, 0.82, 0.85],
]

function noise(seed: number, x: number, y: number) {
  let hash = seed ^ Math.imul(x, 374761393) ^ Math.imul(y, 668265263)
  hash = Math.imul(hash ^ (hash >>> 13), 1274126177)
  return ((hash ^ (hash >>> 16)) >>> 0) / 4294967296
}

export function buildMasonryWall(
  alongX: boolean,
  lo: number,
  hi: number,
  boundary: number,
  inward: number,
  height: number,
  seed: number
): THREE.BufferGeometry {
  const geos: THREE.BufferGeometry[] = []
  const backing = new THREE.BoxGeometry(hi - lo, height, WALL_THICKNESS)
  const backingIndices = Array.from(backing.getIndex()!.array)
  backing.setIndex([
    ...backingIndices.slice(0, 24),
    ...backingIndices.slice(30),
  ])
  backing.translate((lo + hi) / 2, height / 2, -WALL_THICKNESS / 2)
  geos.push(backing)
  const mortar = quadMeshBuilder()
  const addJoint = (x0: number, x1: number, y0: number, y1: number) => {
    if (x1 <= x0 || y1 <= y0) return
    mortar.addQuad(
      new THREE.Vector3(x0, y0, 0),
      new THREE.Vector3(x1, y0, 0),
      new THREE.Vector3(x1, y1, 0),
      new THREE.Vector3(x0, y1, 0),
      new THREE.Vector3(0, 0, 1)
    )
  }

  const wallSeed = seed ^ Math.round(boundary * 7919) ^ (alongX ? 8191 : 0)
  for (let row = 0; row * BRICK_HEIGHT < height; row++) {
    const bottom = Math.max(0.16, row * BRICK_HEIGHT)
    const top = Math.min(height, (row + 1) * BRICK_HEIGHT)
    const shift = (row % 2) * BRICK_WIDTH * 0.5
    for (
      let column = Math.floor((lo - shift) / BRICK_WIDTH);
      column * BRICK_WIDTH + shift < hi;
      column++
    ) {
      const start = column * BRICK_WIDTH + shift
      const left = Math.max(lo, start)
      const right = Math.min(hi, start + BRICK_WIDTH)
      if (right - left < 0.000001) continue
      const edge = left === lo || right === hi || top === height || row === 0
      const sample = noise(wallSeed, column, row)
      const depth = edge
        ? 0
        : sample < 0.02
          ? -0.012
          : sample > 0.96
            ? 0.015 + ((sample - 0.96) / 0.04) * 0.025
            : 0
      const x0 = left + (left === lo ? 0 : 0.0035)
      const x1 = right - (right === hi ? 0 : 0.0035)
      const y0 = bottom + (row === 0 ? 0 : 0.0035)
      const y1 = top - (top === height ? 0 : 0.0035)
      const brick =
        depth === 0
          ? new THREE.PlaneGeometry(x1 - x0, y1 - y0)
          : new THREE.BoxGeometry(x1 - x0, y1 - y0, 1)
      if (depth !== 0)
        brick.setIndex(Array.from(brick.getIndex()!.array).slice(0, 30))
      brick.translate((x0 + x1) / 2, (y0 + y1) / 2, 0)
      const positions = brick.getAttribute('position')
      const normals = brick.getAttribute('normal')
      const uv = brick.getAttribute('uv')
      const face =
        BRICK_FACES[
          Math.floor(noise(wallSeed + 1, column, row) * BRICK_FACES.length)
        ]
      for (let i = 0; i < positions.count; i++) {
        positions.setZ(i, positions.getZ(i) > 0 ? depth : 0)
        const u = THREE.MathUtils.clamp(
          (positions.getX(i) - start) / BRICK_WIDTH,
          0,
          1
        )
        const v = (positions.getY(i) - row * BRICK_HEIGHT) / BRICK_HEIGHT
        const side = Math.abs(normals.getZ(i)) < 0.5
        uv.setXY(
          i,
          (face[0] + (face[2] - face[0]) * (side ? u * 0.2 : u)) / 2,
          (1 - face[3] + (face[3] - face[1]) * v) / 2
        )
      }
      geos.push(brick)
      addJoint(left, x0, bottom, top)
      addJoint(x1, right, bottom, top)
      addJoint(x0, x1, bottom, y0)
      addJoint(x0, x1, y1, top)
    }
  }
  const mortarEntries: GeoEntry[] = []
  mortar.finish(mortarEntries, 0)
  for (const geo of [backing, mortarEntries[0].geo]) {
    const uv = geo.getAttribute('uv')
    for (let i = 0; i < uv.count; i++) uv.setXY(i, 0.235, 0.375)
  }
  geos.push(mortarEntries[0].geo)

  const foot = quadMeshBuilder(0.45)
  const profile = [
    [0, 0.16],
    [0.021, 0.08],
    [0.08, 0.021],
    [0.16, 0],
  ]
  const taperLength = Math.min(0.4, (hi - lo) / 2)
  const stops = [...new Set([lo, lo + taperLength, hi - taperLength, hi])].sort(
    (a, b) => a - b
  )
  for (let i = 0; i < stops.length - 1; i++) {
    for (let p = 0; p < profile.length - 1; p++) {
      const [y0, z0] = profile[p]
      const [y1, z1] = profile[p + 1]
      const point = (x: number, y: number, z: number) =>
        new THREE.Vector3(
          x,
          y,
          z * Math.min(1, (x - lo) / taperLength, (hi - x) / taperLength)
        )
      foot.addQuad(
        point(stops[i], y0, z0),
        point(stops[i + 1], y0, z0),
        point(stops[i + 1], y1, z1),
        point(stops[i], y1, z1),
        new THREE.Vector3(0, z0 - z1, y1 - y0).normalize()
      )
    }
  }
  const footEntries: GeoEntry[] = []
  foot.finish(footEntries, 0)
  geos.push(footEntries[0].geo)
  const geo = mergeGeometries(geos, false)!
  for (const part of geos) part.dispose()
  const positions = geo.getAttribute('position')
  for (let i = 0; i < positions.count; i++) {
    const along = positions.getX(i)
    const across = boundary + inward * positions.getZ(i)
    positions.setXYZ(
      i,
      alongX ? along : across,
      positions.getY(i),
      alongX ? across : along
    )
  }
  if ((alongX && inward < 0) || (!alongX && inward > 0)) {
    const indices = geo.getIndex()!
    for (let i = 0; i < indices.count; i += 3) {
      const first = indices.getX(i)
      indices.setX(i, indices.getX(i + 2))
      indices.setX(i + 2, first)
    }
  }
  geo.computeVertexNormals()
  geo.computeBoundingBox()
  return geo
}

export function masonryFloorBuilder(
  textureIndex: number,
  seed: number,
  clear: (x: number, z: number) => boolean
) {
  const entries: GeoEntry[] = []
  const sides = quadMeshBuilder(DUNGEON_FLOOR_UV_SCALE)
  const v = (x: number, y: number, z: number) => new THREE.Vector3(x, y, z)
  const tile = (points: THREE.Vector2[], lift = 0, tilt = 0) => {
    const cx = points.reduce((sum, p) => sum + p.x, 0) / points.length
    const top = (p: THREE.Vector2) => lift + (p.x - cx) * tilt
    if (points.length === 4) {
      sides.addQuad(
        v(points[0].x, top(points[0]), points[0].y),
        v(points[1].x, top(points[1]), points[1].y),
        v(points[2].x, top(points[2]), points[2].y),
        v(points[3].x, top(points[3]), points[3].y),
        v(-tilt, 1, 0).normalize()
      )
    } else {
      const geo = new THREE.ShapeGeometry(
        new THREE.Shape(points.map((p) => new THREE.Vector2(p.x, -p.y)))
      )
      geo.rotateX(-Math.PI / 2)
      const positions = geo.getAttribute('position')
      const uv = geo.getAttribute('uv')
      for (let i = 0; i < positions.count; i++) {
        positions.setY(i, lift + (positions.getX(i) - cx) * tilt)
        uv.setXY(
          i,
          positions.getX(i) * DUNGEON_FLOOR_UV_SCALE,
          positions.getZ(i) * DUNGEON_FLOOR_UV_SCALE
        )
      }
      geo.computeVertexNormals()
      entries.push({ geo, textureIndex })
    }
    for (let i = 0; i < points.length; i++) {
      const a = points[i]
      const b = points[(i + 1) % points.length]
      sides.addQuad(
        v(a.x, top(a), a.y),
        v(b.x, top(b), b.y),
        v(b.x, -TILE_DEPTH, b.y),
        v(a.x, -TILE_DEPTH, a.y),
        v(b.y - a.y, 0, a.x - b.x).normalize()
      )
    }
  }
  const addCell = (x: number, z: number) => {
    for (let dz = 0; dz < TILES_PER_CELL; dz++) {
      for (let dx = 0; dx < TILES_PER_CELL; dx++) {
        const tx = x * TILES_PER_CELL + dx
        const tz = z * TILES_PER_CELL + dz
        const x0 = tx * TILE_SIZE
        const z0 = tz * TILE_SIZE
        const canBreak = [0.005, TILE_SIZE - 0.005].every((u) =>
          [0.005, TILE_SIZE - 0.005].every((w) => clear(x0 + u, z0 + w))
        )
        const patch = noise(seed + 11, Math.floor(tx / 3), Math.floor(tz / 3))
        const damage = canBreak ? noise(seed, tx, tz) : 1
        const threshold = patch > 0.7 ? 0.42 : 0.1
        const turns = Math.floor(noise(seed + 7, tx, tz) * 4)
        const points = (coords: number[][]) =>
          coords.map(([u, w]) => {
            for (let i = 0; i < turns; i++) [u, w] = [1 - w, u]
            return new THREE.Vector2(x0 + u * TILE_SIZE, z0 + w * TILE_SIZE)
          })
        if (damage >= threshold) {
          tile(
            points([
              [0, 0],
              [1, 0],
              [1, 1],
              [0, 1],
            ])
          )
          continue
        }
        const soil = new THREE.PlaneGeometry(TILE_SIZE, TILE_SIZE, 3, 3)
        soil.rotateX(-Math.PI / 2)
        soil.translate(x0 + TILE_SIZE / 2, -TILE_DEPTH, z0 + TILE_SIZE / 2)
        const positions = soil.getAttribute('position')
        const uv = soil.getAttribute('uv')
        for (let i = 0; i < positions.count; i++) {
          const interior = i % 4 > 0 && i % 4 < 3 && i > 3 && i < 12
          if (interior)
            positions.setY(i, -TILE_DEPTH + noise(seed + i, tx, tz) * 0.014)
          uv.setXY(i, positions.getX(i) * 1.5, positions.getZ(i) * 1.5)
        }
        soil.computeVertexNormals()
        entries.push({ geo: soil, textureIndex: SOIL_TEXTURE })
        if (damage < threshold * 0.35) {
          tile(
            points([
              [0.08, 0.1],
              [0.36, 0.13],
              [0.13, 0.34],
            ]),
            -0.026,
            0.03
          )
        } else if (damage < threshold * 0.7) {
          tile(
            points([
              [0, 0],
              [1, 0],
              [1, 0.32],
              [0.68, 0.48],
              [0.44, 0.72],
              [0, 1],
            ])
          )
          tile(
            points([
              [0.68, 0.81],
              [0.91, 0.72],
              [0.87, 0.94],
            ]),
            -0.015,
            0.04
          )
        } else {
          tile(
            points([
              [0, 0],
              [1, 0],
              [1, 0.37],
              [0.54, 0.57],
              [0, 0.42],
            ])
          )
          tile(
            points([
              [0, 0.46],
              [0.54, 0.61],
              [1, 0.41],
              [1, 1],
              [0, 1],
            ]),
            0.008,
            0.035
          )
        }
      }
    }
  }
  const finish = (target: GeoEntry[]) => {
    sides.finish(entries, textureIndex)
    target.push(...entries)
  }
  return { addCell, finish }
}

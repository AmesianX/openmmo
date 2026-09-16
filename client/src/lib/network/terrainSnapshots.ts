export type TerrainVersion = {
  tile_x: number
  tile_z: number
  version: string
}

export class TerrainSnapshots<T> {
  private active = new Map<string, TerrainVersion>()
  private cache = new Map<string, { tile: T; size: number }>()
  private inflight = new Map<string, Promise<{ tile: T; size: number }>>()
  private timers = new Set<ReturnType<typeof setTimeout>>()
  private cacheSize = 0

  constructor(
    private baseUrl: () => string,
    private decode: (bytes: Uint8Array, version: TerrainVersion) => T,
    private apply: (tile: T) => void,
    private resync: () => void
  ) {}

  set(version: TerrainVersion) {
    const key = `${version.tile_x},${version.tile_z}`
    this.active.set(key, version)
    void this.load(key, version)
  }

  remove(key: string) {
    this.active.delete(key)
  }

  reset() {
    this.active.clear()
    for (const timer of this.timers) clearTimeout(timer)
    this.timers.clear()
  }

  private async load(key: string, version: TerrainVersion) {
    const url = `${this.baseUrl()}/api/terrain/snapshot/full/${version.tile_x}/${version.tile_z}/${version.version}`
    try {
      let result = this.cache.get(url)
      if (!result) {
        let request = this.inflight.get(url)
        if (!request) {
          request = (async () => {
            const response = await fetch(url, {
              signal: AbortSignal.timeout(15000),
            })
            if (!response.ok) throw new Error(String(response.status))
            const bytes = new Uint8Array(await response.arrayBuffer())
            return { tile: this.decode(bytes, version), size: bytes.byteLength }
          })()
          this.inflight.set(url, request)
        }
        try {
          result = await request
        } finally {
          if (this.inflight.get(url) === request) this.inflight.delete(url)
        }
        if (!this.cache.has(url) && result.size <= 8 * 1024 * 1024) {
          this.cache.set(url, result)
          this.cacheSize += result.size
          while (this.cacheSize > 8 * 1024 * 1024) {
            const oldest = this.cache.entries().next().value!
            this.cache.delete(oldest[0])
            this.cacheSize -= oldest[1].size
          }
        }
      } else {
        this.cache.delete(url)
        this.cache.set(url, result)
      }
      if (this.active.get(key) === version) this.apply(result.tile)
    } catch (error) {
      if (this.active.get(key) !== version) return
      if (error instanceof Error && error.message === '409') {
        this.resync()
        return
      }
      const timer = setTimeout(() => {
        this.timers.delete(timer)
        if (this.active.get(key) === version) void this.load(key, version)
      }, 1000)
      this.timers.add(timer)
    }
  }
}

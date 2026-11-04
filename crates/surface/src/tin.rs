//! The TIN type every surface module shares. Changing it needs both owners to agree (see
//! `docs/agents/slice-2-tin.md`).

/// A triangulated irregular network in the drawing's own units and coordinate system.
///
/// `vertices` are `[x, y, z]` (x = easting, y = northing, z = elevation). `triangles` index into
/// `vertices`, counter-clockwise seen from above. Every index is in range and no triangle repeats
/// a vertex; producers guarantee this and consumers may rely on it, but must still use `get()`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tin {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

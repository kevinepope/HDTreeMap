//! [`Bins`]: which rectangles touch which cell of a uniform grid, in compressed-row form
//! (one `Vec<u32>` of rectangle indices, one offset per cell).
//!
//! Hit testing (`layout/hit.rs`) uses square cells to find the few rectangles under a
//! point; the rasteriser (`render/raster.rs`) uses full-width cells, one per band of rows,
//! so each band paints only the rectangles that reach into it.

/// Rectangle indices grouped by grid cell. See the module docs.
#[derive(Clone, Debug, Default)]
pub struct Bins {
    /// `starts[c]..starts[c + 1]` indexes `items` for cell `c`.
    starts: Vec<u32>,
    items: Vec<u32>,
}

impl Bins {
    /// Group `count` rectangles into `cells` cells. `cells_of(i)` lists rectangle `i`'s
    /// cells (it is called twice per rectangle, so it must be deterministic); out-of-range
    /// cells are ignored.
    pub fn build<I>(count: usize, cells: usize, cells_of: impl Fn(usize) -> I) -> Self
    where
        I: Iterator<Item = usize>,
    {
        let mut starts = vec![0u32; cells + 1];
        for i in 0..count {
            for c in cells_of(i) {
                if c < cells {
                    starts[c + 1] += 1;
                }
            }
        }
        for c in 0..cells {
            starts[c + 1] += starts[c];
        }
        let mut fill = starts.clone();
        let mut items = vec![0u32; starts[cells] as usize];
        for i in 0..count {
            for c in cells_of(i) {
                if c < cells {
                    items[fill[c] as usize] = i as u32;
                    fill[c] += 1;
                }
            }
        }
        Self { starts, items }
    }

    /// Number of cells.
    pub fn cells(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }

    /// The rectangles touching cell `c`, in index order (empty for an unknown cell).
    pub fn cell(&self, c: usize) -> &[u32] {
        match (self.starts.get(c), self.starts.get(c + 1)) {
            (Some(&from), Some(&to)) => &self.items[from as usize..to as usize],
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_land_in_their_cells_in_order() {
        // Three rectangles over four cells: 0 → {0, 1}, 1 → {1}, 2 → {3, 9 (out of range)}.
        let spans: [&[usize]; 3] = [&[0, 1], &[1], &[3, 9]];
        let bins = Bins::build(3, 4, |i| spans[i].iter().copied());
        assert_eq!(bins.cells(), 4);
        assert_eq!(bins.cell(0), &[0]);
        assert_eq!(bins.cell(1), &[0, 1]);
        assert_eq!(bins.cell(2), &[] as &[u32]);
        assert_eq!(bins.cell(3), &[2]);
        assert_eq!(bins.cell(4), &[] as &[u32]);
    }
}

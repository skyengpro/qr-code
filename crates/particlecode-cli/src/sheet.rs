//! Contact sheets: many rendered frames blitted into one grid image.

use anyhow::{Context, Result, anyhow};
use particlecode_core::ShapeChoice;
use particlecode_core::profile::{Profile, Rgb, colors};
use particlecode_render::{RenderConfig, render};
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

use crate::spec::SessionSpec;

/// Lay `tiles` out row by row in a grid with `columns` columns and `gap` pixels between
/// (and around) the tiles. Every cell is as large as the largest tile; smaller tiles are
/// centred. Returns `None` for no tiles, zero columns or an image too large to allocate.
pub fn contact_sheet(
    tiles: &[Pixmap],
    columns: usize,
    gap: u32,
    background: Rgb,
) -> Option<Pixmap> {
    if tiles.is_empty() || columns == 0 {
        return None;
    }
    let cell_w = tiles.iter().map(Pixmap::width).max()?;
    let cell_h = tiles.iter().map(Pixmap::height).max()?;
    let columns = columns.min(tiles.len());
    let rows = tiles.len().div_ceil(columns);
    let width = (columns as u32)
        .checked_mul(cell_w + gap)?
        .checked_add(gap)?;
    let height = (rows as u32).checked_mul(cell_h + gap)?.checked_add(gap)?;
    let mut sheet = Pixmap::new(width, height)?;
    sheet.fill(Color::from_rgba8(
        background.0,
        background.1,
        background.2,
        255,
    ));
    for (i, tile) in tiles.iter().enumerate() {
        let (col, row) = ((i % columns) as u32, (i / columns) as u32);
        let x = gap + col * (cell_w + gap) + (cell_w - tile.width()) / 2;
        let y = gap + row * (cell_h + gap) + (cell_h - tile.height()) / 2;
        sheet.draw_pixmap(
            x as i32,
            y as i32,
            tile.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
    Some(sheet)
}

/// One frame of `payload` per shape in `shapes`, each rendered `tile_px` square, laid out
/// with [`contact_sheet`]. Every tile uses the same payload and identity, so the sheet shows
/// what the mask alone does to the field.
pub fn shape_sheet(
    shapes: &[u8],
    payload: &[u8],
    profile: Profile,
    tile_px: u32,
    columns: usize,
    gap: u32,
) -> Result<Pixmap> {
    let cfg = RenderConfig {
        size_px: tile_px,
        ..RenderConfig::default()
    };
    let tiles = shapes
        .iter()
        .map(|&id| {
            let spec = SessionSpec {
                profile,
                shape: ShapeChoice::Require(id),
                session_id: Some(0x5EED),
                key: Some([0x42; 32]),
                clock_origin: Some(1_780_000_000),
                ..SessionSpec::default()
            };
            let session = spec.build(payload).with_context(|| format!("shape {id}"))?;
            Ok(render(&session.frame(0), &cfg))
        })
        .collect::<Result<Vec<_>>>()?;
    contact_sheet(&tiles, columns, gap, colors::BACKGROUND)
        .ok_or_else(|| anyhow!("cannot lay out {} tiles", tiles.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(w: u32, h: u32, c: Color) -> Pixmap {
        let mut p = Pixmap::new(w, h).expect("pixmap");
        p.fill(c);
        p
    }

    #[test]
    fn grid_dimensions_and_placement() {
        let red = Color::from_rgba8(255, 0, 0, 255);
        let tiles: Vec<_> = (0..5).map(|_| tile(10, 10, red)).collect();
        let sheet = contact_sheet(&tiles, 3, 2, Rgb(0, 0, 255)).expect("sheet");
        assert_eq!((sheet.width(), sheet.height()), (3 * 12 + 2, 2 * 12 + 2));
        let px = |x, y| {
            let c = sheet.pixel(x, y).expect("in range").demultiply();
            (c.red(), c.green(), c.blue())
        };
        assert_eq!(px(0, 0), (0, 0, 255), "gap is background");
        assert_eq!(px(2, 2), (255, 0, 0), "first tile");
        assert_eq!(px(2 + 12, 2 + 12), (255, 0, 0), "second row, second cell");
        assert_eq!(px(2 + 12 * 2, 2 + 12), (0, 0, 255), "sixth cell is empty");
    }

    #[test]
    fn smaller_tiles_are_centred() {
        let big = tile(10, 10, Color::from_rgba8(0, 255, 0, 255));
        let small = tile(4, 4, Color::from_rgba8(255, 0, 0, 255));
        let sheet = contact_sheet(&[big, small], 2, 0, Rgb(0, 0, 0)).expect("sheet");
        let c = sheet.pixel(10 + 3, 3).expect("in range").demultiply();
        assert_eq!(c.red(), 255);
        let c = sheet.pixel(10, 0).expect("in range").demultiply();
        assert_eq!(c.red(), 0);
    }

    #[test]
    fn shape_sheet_renders_every_shape() {
        let sheet = shape_sheet(&[0, 28, 17], &[1, 2, 3], Profile::S, 60, 2, 4).expect("sheet");
        assert_eq!((sheet.width(), sheet.height()), (2 * 64 + 4, 2 * 64 + 4));
        assert!(shape_sheet(&[200], &[1], Profile::S, 60, 1, 0).is_err());
        assert!(shape_sheet(&[], &[1], Profile::S, 60, 1, 0).is_err());
    }

    #[test]
    fn degenerate_inputs() {
        assert!(contact_sheet(&[], 3, 1, Rgb(0, 0, 0)).is_none());
        assert!(contact_sheet(&[tile(2, 2, Color::BLACK)], 0, 1, Rgb(0, 0, 0)).is_none());
    }
}

//! Adding a checked shape to a [`ShapeTable`].

use std::fmt;

use particlecode_core::shapes::{FIRST_CUSTOM, ShapeError, ShapeTable};

use crate::raster::ShapeInput;
use crate::rules::{Report, check};

/// Why a shape was not added.
#[derive(Debug)]
pub enum BuildError {
    /// Ids below 32 belong to the built-in table; `--force` does not change that.
    ReservedId(u8),
    /// Rules 2 to 4 fail (pass `force` to add the shape anyway).
    RulesFailed(Box<Report>),
    /// The table refused the shape (duplicate id, name too long, ...).
    Table(ShapeError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::ReservedId(id) => write!(
                f,
                "shape id {id} is reserved for the built-in table; custom shapes use \
                 {FIRST_CUSTOM}..=255"
            ),
            BuildError::RulesFailed(_) => {
                write!(f, "the shape fails the rules above; fix it or pass --force")
            }
            BuildError::Table(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for BuildError {}

/// Check `input` and insert it into `table` as shape `id` named `name`.
///
/// The id is validated first; then rules 2 to 4 are checked on all profiles and, unless
/// `force`, a failing shape is refused. Returns the report of the accepted shape.
pub fn add_shape(
    table: &mut ShapeTable,
    id: u8,
    name: &str,
    input: &ShapeInput,
    force: bool,
) -> Result<Report, BuildError> {
    if id < FIRST_CUSTOM {
        return Err(BuildError::ReservedId(id));
    }
    let report = check(input);
    if !report.passed() && !force {
        return Err(BuildError::RulesFailed(Box::new(report)));
    }
    table
        .insert(id, name, input.masks.clone())
        .map_err(BuildError::Table)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::ViewBox;

    fn square(lo: u32, hi: u32) -> ShapeInput {
        let d = format!("M {lo} {lo} L {hi} {lo} L {hi} {hi} L {lo} {hi} Z");
        ShapeInput::from_path_data(&d, &ViewBox::DESIGN, None, "square").unwrap()
    }

    #[test]
    fn refuses_builtin_ids_even_when_forced() {
        let mut table = ShapeTable::builtin();
        let input = square(100, 900);
        for force in [false, true] {
            let err = add_shape(&mut table, 5, "x", &input, force).unwrap_err();
            assert!(matches!(err, BuildError::ReservedId(5)), "{err}");
        }
        assert_eq!(table.name(5), Some("D"));
    }

    #[test]
    fn refuses_failing_shapes_unless_forced() {
        let mut table = ShapeTable::builtin();
        let tiny = square(450, 550);
        let err = add_shape(&mut table, 40, "tiny", &tiny, false).unwrap_err();
        assert!(matches!(err, BuildError::RulesFailed(_)));
        assert!(!table.contains(40));
        add_shape(&mut table, 40, "tiny", &tiny, true).unwrap();
        assert_eq!(table.name(40), Some("tiny"));
    }

    #[test]
    fn refuses_duplicates() {
        let mut table = ShapeTable::builtin();
        let input = square(100, 900);
        add_shape(&mut table, 33, "a", &input, false).unwrap();
        let err = add_shape(&mut table, 33, "b", &input, false).unwrap_err();
        assert!(matches!(
            err,
            BuildError::Table(ShapeError::DuplicateId(33))
        ));
    }
}

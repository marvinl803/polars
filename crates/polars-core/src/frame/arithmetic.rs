use std::ops::{Add, Div, Mul, Rem, Sub};

use rayon::prelude::*;

use crate::POOL;
use crate::prelude::*;
use crate::utils::try_get_supertype;

/// Get the supertype that is valid for all columns in the [`DataFrame`].
/// This reduces casting of the rhs in arithmetic.
fn get_supertype_all(df: &DataFrame, rhs: &Series) -> PolarsResult<DataType> {
    df.columns.iter().try_fold(rhs.dtype().clone(), |dt, s| {
        try_get_supertype(s.dtype(), &dt)
    })
}

macro_rules! impl_arithmetic {
    ($self:expr, $rhs:expr, $operand:expr) => {{
        let st = get_supertype_all($self, $rhs)?;
        let rhs = $rhs.cast(&st)?;
        let cols = POOL.install(|| {
            $self
                .par_materialized_column_iter()
                .map(|s| $operand(&s.cast(&st)?, &rhs))
                .map(|s| s.map(Column::from))
                .collect::<PolarsResult<_>>()
        })?;
        Ok(unsafe { DataFrame::new_no_checks($self.height(), cols) })
    }};
}

impl Add<&Series> for &DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn add(self, rhs: &Series) -> Self::Output {
        impl_arithmetic!(self, rhs, std::ops::Add::add)
    }
}

impl Add<&Series> for DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn add(self, rhs: &Series) -> Self::Output {
        (&self).add(rhs)
    }
}

impl Sub<&Series> for &DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn sub(self, rhs: &Series) -> Self::Output {
        impl_arithmetic!(self, rhs, std::ops::Sub::sub)
    }
}

impl Sub<&Series> for DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn sub(self, rhs: &Series) -> Self::Output {
        (&self).sub(rhs)
    }
}

impl Mul<&Series> for &DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn mul(self, rhs: &Series) -> Self::Output {
        impl_arithmetic!(self, rhs, std::ops::Mul::mul)
    }
}

impl Mul<&Series> for DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn mul(self, rhs: &Series) -> Self::Output {
        (&self).mul(rhs)
    }
}

impl Div<&Series> for &DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn div(self, rhs: &Series) -> Self::Output {
        impl_arithmetic!(self, rhs, std::ops::Div::div)
    }
}

impl Div<&Series> for DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn div(self, rhs: &Series) -> Self::Output {
        (&self).div(rhs)
    }
}

impl Rem<&Series> for &DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn rem(self, rhs: &Series) -> Self::Output {
        impl_arithmetic!(self, rhs, std::ops::Rem::rem)
    }
}

impl Rem<&Series> for DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn rem(self, rhs: &Series) -> Self::Output {
        (&self).rem(rhs)
    }
}

impl DataFrame {
    fn binary_aligned(
        &self,
        other: &DataFrame,
        f: &(dyn Fn(&Series, &Series) -> PolarsResult<Series> + Sync + Send),
    ) -> PolarsResult<DataFrame> {
        let max_len = std::cmp::max(self.height(), other.height());
        let max_width = std::cmp::max(self.width(), other.width());
        let cols = self
            .get_columns()
            .par_iter()
            .zip(other.get_columns().par_iter())
            .map(|(l, r)| {
                let l = l.as_materialized_series();
                let r = r.as_materialized_series();

                let diff_l = max_len - l.len();
                let diff_r = max_len - r.len();

                let st = try_get_supertype(l.dtype(), r.dtype())?;
                let mut l = l.cast(&st)?;
                let mut r = r.cast(&st)?;

                if diff_l > 0 {
                    l = l.extend_constant(AnyValue::Null, diff_l)?;
                };
                if diff_r > 0 {
                    r = r.extend_constant(AnyValue::Null, diff_r)?;
                };

                f(&l, &r).map(Column::from)
            });
        let mut cols = POOL.install(|| cols.collect::<PolarsResult<Vec<_>>>())?;

        let col_len = cols.len();
        if col_len < max_width {
            let df = if col_len < self.width() { self } else { other };

            for i in col_len..max_len {
                let s = &df.get_columns().get(i).ok_or_else(|| polars_err!(InvalidOperation: "cannot do arithmetic on DataFrames with shapes: {:?} and {:?}", self.shape(), other.shape()))?;
                let name = s.name();
                let dtype = s.dtype();

                // trick to fill a series with nulls
                let vals: &[Option<i32>] = &[None];
                let s = Series::new(name.clone(), vals).cast(dtype)?;
                cols.push(s.new_from_index(0, max_len).into())
            }
        }
        DataFrame::new(cols)
    }
}

impl Add<&DataFrame> for &DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn add(self, rhs: &DataFrame) -> Self::Output {
        self.binary_aligned(rhs, &|a, b| a + b)
    }
}

impl Sub<&DataFrame> for &DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn sub(self, rhs: &DataFrame) -> Self::Output {
        self.binary_aligned(rhs, &|a, b| a - b)
    }
}

impl Div<&DataFrame> for &DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn div(self, rhs: &DataFrame) -> Self::Output {
        self.binary_aligned(rhs, &|a, b| a / b)
    }
}

impl Mul<&DataFrame> for &DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn mul(self, rhs: &DataFrame) -> Self::Output {
        self.binary_aligned(rhs, &|a, b| a * b)
    }
}

impl Rem<&DataFrame> for &DataFrame {
    type Output = PolarsResult<DataFrame>;

    fn rem(self, rhs: &DataFrame) -> Self::Output {
        self.binary_aligned(rhs, &|a, b| a % b)
    }
}

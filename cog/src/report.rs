//! Collecting the results of work that carries on past individual failures.

use rayon::iter::{Either, FromParallelIterator, IntoParallelIterator, ParallelIterator};

use crate::error::{Error, Result};

/// What succeeded and what failed, when one failure doesn't stop the rest.
///
/// Collect an iterator of `Result<T>` into one, in parallel or not, and the
/// order of each list follows the order of the iterator.
#[derive(Debug)]
pub struct Report<T> {
    pub done: Vec<T>,
    pub errors: Vec<Error>,
}

impl<T> Report<T> {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

impl<T> Default for Report<T> {
    fn default() -> Self {
        Self {
            done: Vec::new(),
            errors: Vec::new(),
        }
    }
}

impl<T> Extend<Result<T>> for Report<T> {
    fn extend<I: IntoIterator<Item = Result<T>>>(&mut self, iter: I) {
        for result in iter {
            match result {
                Ok(item) => self.done.push(item),
                Err(e) => self.errors.push(e),
            }
        }
    }
}

impl<T> FromIterator<Result<T>> for Report<T> {
    fn from_iter<I: IntoIterator<Item = Result<T>>>(iter: I) -> Self {
        let mut report = Self::default();
        report.extend(iter);
        report
    }
}

impl<T: Send> FromParallelIterator<Result<T>> for Report<T> {
    fn from_par_iter<I>(iter: I) -> Self
    where
        I: IntoParallelIterator<Item = Result<T>>,
    {
        let (done, errors) = iter.into_par_iter().partition_map(|result| match result {
            Ok(item) => Either::Left(item),
            Err(e) => Either::Right(e),
        });
        Self { done, errors }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn results() -> Vec<Result<u32>> {
        (0..100)
            .map(|i| {
                if i % 10 == 0 {
                    Err(Error::NotADirectory {
                        path: PathBuf::from(i.to_string()),
                    })
                } else {
                    Ok(i)
                }
            })
            .collect()
    }

    #[test]
    fn serial_and_parallel_collect_agree_and_keep_order() {
        let serial: Report<u32> = results().into_iter().collect();
        let parallel: Report<u32> = results().into_par_iter().collect();

        assert_eq!(serial.done, parallel.done);
        assert!(serial.done.is_sorted());
        assert_eq!(serial.done.len(), 90);
        assert_eq!(parallel.errors.len(), 10);
        assert!(parallel.has_errors());
    }
}

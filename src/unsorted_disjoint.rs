// !!!cmk make the names consistent, start/lower vs stop/upper/end/...
// !!!cmk replace OptionRange with Option<(T, T)>

use num_traits::Zero;
use std::cmp::{max, min};

use crate::{Integer, SafeSubtract, SortedDisjoint};

pub struct UnsortedDisjoint<T, I>
where
    T: Integer,
    I: Iterator<Item = (T, T)>,
{
    iter: I,
    range: Option<(T, T)>,
    two: T,
}

impl<T, I> From<I> for UnsortedDisjoint<T, I>
where
    T: Integer,
    I: Iterator<Item = (T, T)>, // cmk should this be IntoIterator?
{
    fn from(iter: I) -> Self {
        UnsortedDisjoint {
            iter,
            range: None,
            two: T::one() + T::one(),
        }
    }
}

impl<T, I> Iterator for UnsortedDisjoint<T, I>
where
    T: Integer,
    I: Iterator<Item = (T, T)>,
{
    type Item = (T, T);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((lower, upper)) = self.iter.next() {
            assert!(lower <= upper && upper <= T::max_value2()); // !!!cmk0 raise error on panic?
            if let Some((self_lower, self_upper)) = self.range {
                if (lower >= self.two && lower - self.two >= self_upper)
                    || (self_lower >= self.two && self_lower - self.two >= upper)
                {
                    let result = Some((self_lower, self_upper));
                    self.range = Some((lower, upper));
                    result
                } else {
                    self.range = Some((min(self_lower, lower), max(self_upper, upper)));
                    self.next()
                }
            } else {
                self.range = Some((lower, upper));
                self.next()
            }
        } else if let Some((start, stop)) = self.range {
            self.range = None;
            Some((start, stop))
        } else {
            None
        }
    }
}

pub struct SortedDisjointWithLenSoFar<T, I>
where
    T: Integer,
    I: Iterator<Item = (T, T)> + SortedDisjoint,
{
    iter: I,
    len: <T as SafeSubtract>::Output,
}

impl<T: Integer, I> From<I> for SortedDisjointWithLenSoFar<T, I>
where
    I: Iterator<Item = (T, T)> + SortedDisjoint,
{
    fn from(iter: I) -> Self {
        SortedDisjointWithLenSoFar {
            iter,
            len: <T as SafeSubtract>::Output::zero(),
        }
    }
}

impl<T: Integer, I> SortedDisjointWithLenSoFar<T, I>
where
    I: Iterator<Item = (T, T)> + SortedDisjoint,
{
    pub fn len(&self) -> <T as SafeSubtract>::Output {
        self.len.clone()
    }
}

impl<T: Integer, I> Iterator for SortedDisjointWithLenSoFar<T, I>
where
    I: Iterator<Item = (T, T)> + SortedDisjoint,
{
    type Item = (T, T);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((start, stop)) = self.iter.next() {
            debug_assert!(start <= stop && stop <= T::max_value2());
            self.len += T::safe_subtract_inclusive(stop, start);
            Some((start, stop))
        } else {
            None
        }
    }
}
impl<T: Integer, I> SortedDisjoint for SortedDisjointWithLenSoFar<T, I> where
    I: Iterator<Item = (T, T)> + SortedDisjoint
{
}
// https://docs.rs/range_bounds_map/latest/range_bounds_map/range_bounds_set/struct.RangeBoundsSet.html
// Here are some relevant crates I found whilst searching around the topic area:

// https://crates.io/crates/sorted-iter
//    cmk0 Look at sorted-iter's note about exporting.
//    cmk0 Look at sorted-iter's note about their testing tool.
// https://docs.rs/rangemap Very similar to this crate but can only use Ranges and RangeInclusives as keys in it's map and set structs (separately).
// https://docs.rs/btree-range-map
// https://docs.rs/ranges Cool library for fully-generic ranges (unlike std::ops ranges), along with a Ranges data structure for storing them (Vec-based unfortunately)
// https://docs.rs/intervaltree Allows overlapping intervals but is immutable unfortunately
// https://docs.rs/nonoverlapping_interval_tree Very similar to rangemap except without a gaps() function and only for Ranges and not RangeInclusives. And also no fancy coalescing functions.
// https://docs.rs/unbounded-interval-tree A data structure based off of a 2007 published paper! It supports any RangeBounds as keys too, except it is implemented with a non-balancing Box<Node> based tree, however it also supports overlapping RangeBounds which my library does not.
// https://docs.rs/rangetree I'm not entirely sure what this library is or isn't, but it looks like a custom red-black tree/BTree implementation used specifically for a Range Tree. Interesting but also quite old (5 years) and uses unsafe.
// https://docs.rs/btree-range-map/latest/btree_range_map/
// Related: https://lib.rs/crates/iset
// https://lib.rs/crates/interval_tree
// https://lib.rs/crates/range-set
// https://lib.rs/crates/rangemap
// https://lib.rs/crates/ranges
// https://lib.rs/crates/nonoverlapping_interval_tree
// https://stackoverflow.com/questions/30540766/how-can-i-add-new-methods-to-iterator
// !!!cmk0 how could you write your own subtraction that subtracted many sets from one set via iterators?
// cmk rules: When should use Iterator and when IntoIterator?
// cmk rules: When should use: from_iter, from, new from_something?
// !!! cmk rule: Don't have a function and a method. Pick one (method)
// !!!cmk rule: Follow the rules of good API design including accepting almost any type of input
// cmk rule: don't create an assign method if it is not more efficient
// cmk00000 benchmark.

mod integer;
pub mod not_iter;
pub mod sorted_disjoint_iter;
mod tests;
pub mod unsorted_disjoint;

use gen_ops::gen_ops_ex;
use itertools::Itertools;
use itertools::KMergeBy;
use itertools::MergeBy;
use itertools::Tee;
use not_iter::NotIter;
use num_traits::ops::overflowing::OverflowingSub;
use num_traits::Zero;
// cmk0 move rand to dev-dependencies
use sorted_disjoint_iter::SortedDisjointIter;
use std::cmp::max;
use std::collections::btree_map;
use std::collections::BTreeMap;
use std::convert::From;
use std::fmt;
use std::fmt::Debug;
use std::ops;
use std::ops::RangeInclusive;
use std::ops::Sub;
use std::str::FromStr;
use thiserror::Error as ThisError;
use unsorted_disjoint::SortedDisjointWithLenSoFar;
use unsorted_disjoint::UnsortedDisjoint;

// cmk rule: Support Send and Sync (what about Clone (Copy?) and ExactSizeIterator?)

// cmk rule: Define your element type
pub trait Integer:
    num_integer::Integer
    + FromStr
    + fmt::Display
    + fmt::Debug
    + std::iter::Sum
    + num_traits::NumAssignOps
    + FromStr
    + Copy
    + num_traits::Bounded
    + num_traits::NumCast
    + Send
    + Sync
    + OverflowingSub
{
    type SafeLen: std::hash::Hash
        + num_integer::Integer
        + std::ops::AddAssign
        + std::ops::SubAssign
        + Clone
        + PartialEq
        + Eq
        + PartialOrd
        + Ord
        + Send
        + Default
        + fmt::Debug
        + fmt::Display;
    fn safe_inclusive_len(range_inclusive: &RangeInclusive<Self>) -> <Self as Integer>::SafeLen;

    fn max_value2() -> Self {
        Self::max_value()
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct RangeSetInt<T: Integer> {
    len: <T as Integer>::SafeLen,
    btree_map: BTreeMap<T, T>,
}

impl<T: Integer> fmt::Debug for RangeSetInt<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.ranges().to_string())
    }
}

impl<T: Integer> fmt::Display for RangeSetInt<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.ranges().to_string())
    }
}

impl<T: Integer> RangeSetInt<T> {
    // If the user asks for an iter, we give them a borrow to a Ranges iterator
    // and we iterate that one integer at a time.
    pub fn iter(&self) -> Iter<T, impl Iterator<Item = RangeInclusive<T>> + SortedDisjoint + '_> {
        Iter {
            current: T::zero(),
            option_range_inclusive: None,
            iter: self.ranges(),
        }
    }
}

impl<'a, T: Integer + 'a> RangeSetInt<T> {
    pub fn union<I>(input: I) -> Self
    where
        I: IntoIterator<Item = &'a RangeSetInt<T>>,
    {
        union(input.into_iter().map(|x| x.ranges())).into()
    }

    pub fn intersection<I>(input: I) -> Self
    where
        I: IntoIterator<Item = &'a RangeSetInt<T>>,
    {
        intersection(input.into_iter().map(|x| x.ranges())).into()
    }
}

impl<T: Integer> RangeSetInt<T> {
    /// !!! cmk understand the 'where for'
    /// !!! cmk understand the operator 'Sub'
    fn _len_slow(&self) -> <T as Integer>::SafeLen
    where
        for<'a> &'a T: Sub<&'a T, Output = T>,
    {
        self.btree_map
            .iter()
            .fold(<T as Integer>::SafeLen::zero(), |acc, (start, stop)| {
                acc + T::safe_inclusive_len(&(*start..=*stop))
            })
    }

    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use range_set_int::RangeSetInt;
    ///
    /// let mut a = RangeSetInt::from([1..=3]);
    /// let mut b = RangeSetInt::from([3..=5]);
    ///
    /// a.append(&mut b);
    ///
    /// assert_eq!(a.len(), 5usize);
    /// assert_eq!(b.len(), 0usize);
    ///
    /// assert!(a.contains(1));
    /// assert!(a.contains(2));
    /// assert!(a.contains(3));
    /// assert!(a.contains(4));
    /// assert!(a.contains(5));
    ///
    /// ```
    /// cmk add a note about the performance compared
    /// to bitor
    pub fn append(&mut self, other: &mut Self) {
        for range_inclusive in other.ranges() {
            self.internal_add(range_inclusive);
        }
        other.clear();
    }

    pub fn clear(&mut self) {
        self.btree_map.clear();
        self.len = <T as Integer>::SafeLen::zero();
    }

    /// Returns `true` if the set contains an element equal to the value.
    ///
    /// # Examples
    ///
    /// ```
    /// use range_set_int::RangeSetInt;
    ///
    /// let set = RangeSetInt::from([1, 2, 3]);
    /// assert_eq!(set.contains(1), true);
    /// assert_eq!(set.contains(4), false);
    /// ```
    pub fn contains(&self, value: T) -> bool {
        self.btree_map
            .range(..=value)
            .next_back()
            .map_or(false, |(_, stop)| value <= *stop)
    }

    fn delete_extra(&mut self, internal_inclusive: &RangeInclusive<T>) {
        let (start, stop) = internal_inclusive.clone().into_inner();
        let mut after = self.btree_map.range_mut(start..);
        let (start_after, stop_after) = after.next().unwrap(); // there will always be a next
        debug_assert!(start == *start_after && stop == *stop_after); // real assert
                                                                     // !!!cmk would be nice to have a delete_range function
        let mut stop_new = stop;
        let delete_list = after
            .map_while(|(start_delete, stop_delete)| {
                // must check this in two parts to avoid overflow
                if *start_delete <= stop || *start_delete <= stop + T::one() {
                    stop_new = max(stop_new, *stop_delete);
                    self.len -= T::safe_inclusive_len(&(*start_delete..=*stop_delete));
                    Some(*start_delete)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if stop_new > stop {
            self.len += T::safe_inclusive_len(&(stop..=stop_new - T::one()));
            *stop_after = stop_new;
        }
        for start in delete_list {
            self.btree_map.remove(&start);
        }
    }

    pub fn insert(&mut self, item: T) {
        self.internal_add(item..=item);
    }

    // https://stackoverflow.com/questions/49599833/how-to-find-next-smaller-key-in-btreemap-btreeset
    // https://stackoverflow.com/questions/35663342/how-to-modify-partially-remove-a-range-from-a-btreemap
    fn internal_add(&mut self, range_inclusive: RangeInclusive<T>) {
        let (start, stop) = range_inclusive.clone().into_inner();
        if stop < start {
            return;
        }
        assert!(stop <= T::max_value2()); //cmk0 panic
                                          // !!! cmk would be nice to have a partition_point function that returns two iterators
        let mut before = self.btree_map.range_mut(..=start).rev();
        if let Some((start_before, stop_before)) = before.next() {
            // Must check this in two parts to avoid overflow
            if *stop_before < start && *stop_before + T::one() < start {
                self.internal_add2(&range_inclusive);
            } else if *stop_before < stop {
                self.len += T::safe_inclusive_len(&(*stop_before..=stop - T::one()));
                *stop_before = stop;
                let start_before = *start_before;
                self.delete_extra(&(start_before..=stop));
            } else {
                // completely contained, so do nothing
            }
        } else {
            self.internal_add2(&range_inclusive);
        }
    }

    fn internal_add2(&mut self, internal_inclusive: &RangeInclusive<T>) {
        let (start, stop) = internal_inclusive.clone().into_inner();
        let was_there = self.btree_map.insert(start, stop);
        debug_assert!(was_there.is_none()); // real assert
        self.delete_extra(internal_inclusive);
        self.len += T::safe_inclusive_len(internal_inclusive);
    }

    pub fn len(&self) -> <T as Integer>::SafeLen {
        self.len.clone()
    }

    pub fn new() -> RangeSetInt<T> {
        RangeSetInt {
            btree_map: BTreeMap::new(),
            len: <T as Integer>::SafeLen::zero(),
        }
    }

    pub fn ranges(&self) -> Ranges<'_, T> {
        let ranges = Ranges {
            iter: self.btree_map.iter(),
        };
        ranges
    }

    pub fn ranges_len(&self) -> usize {
        self.btree_map.len()
    }
}

#[derive(Clone)]
pub struct Ranges<'a, T: Integer> {
    iter: btree_map::Iter<'a, T, T>,
}

impl<'a, T: Integer> AsRef<Ranges<'a, T>> for Ranges<'a, T> {
    fn as_ref(&self) -> &Self {
        // Self is Ranges<'a>, the type for which we impl AsRef
        self
    }
}

// Ranges (one of the iterators from RangeSetInt) is SortedDisjoint
impl<T: Integer> SortedStarts for Ranges<'_, T> {}
impl<T: Integer> SortedDisjoint for Ranges<'_, T> {}
// If the iterator inside a BitOrIter is SortedStart, the output will be SortedDisjoint
impl<T: Integer, I: Iterator<Item = RangeInclusive<T>> + SortedStarts> SortedStarts
    for SortedDisjointIter<T, I>
{
}
impl<T: Integer, I: Iterator<Item = RangeInclusive<T>> + SortedStarts> SortedDisjoint
    for SortedDisjointIter<T, I>
{
}
// If the iterator inside NotIter is SortedDisjoint, the output will be SortedDisjoint
impl<T: Integer, I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint> SortedStarts
    for NotIter<T, I>
{
}
impl<T: Integer, I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint> SortedDisjoint
    for NotIter<T, I>
{
}
// If the iterator inside Tee is SortedDisjoint, the output will be SortedDisjoint
impl<T: Integer, I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint> SortedStarts for Tee<I> {}
impl<T: Integer, I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint> SortedDisjoint for Tee<I> {}

impl<T: Integer> ExactSizeIterator for Ranges<'_, T> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

// Range's iterator is just the inside BTreeMap iterator as values
impl<'a, T: Integer> Iterator for Ranges<'a, T> {
    type Item = RangeInclusive<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(start, stop)| *start..=*stop)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

// We create a RangeSetInt from an iterator of integers or integer ranges by
// 1. turning them into a BitOrIter (internally, it collects into intervals and sorts by start).
// 2. Turning the SortedDisjoint into a BTreeMap.
impl<T: Integer> FromIterator<T> for RangeSetInt<T> {
    fn from_iter<I>(into_iter: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        into_iter.into_iter().map(|x| x..=x).collect()
    }
}

// cmk rules: Follow Rust conventions. For example this as empty let cmk = 1..=-1; we do the same
impl<T: Integer> FromIterator<RangeInclusive<T>> for RangeSetInt<T> {
    fn from_iter<I>(into_iter: I) -> Self
    where
        I: IntoIterator<Item = RangeInclusive<T>>,
    {
        let sorted_disjoint_iter: SortedDisjointIter<T, _> = into_iter.into_iter().collect();
        sorted_disjoint_iter.into()
    }
}

impl<T: Integer, const N: usize> From<[RangeInclusive<T>; N]> for RangeSetInt<T> {
    fn from(arr: [RangeInclusive<T>; N]) -> Self {
        arr.as_slice().into()
    }
}

impl<T: Integer> From<&[RangeInclusive<T>]> for RangeSetInt<T> {
    fn from(slice: &[RangeInclusive<T>]) -> Self {
        slice.iter().cloned().collect()
    }
}

impl<T: Integer, const N: usize> From<[T; N]> for RangeSetInt<T> {
    fn from(arr: [T; N]) -> Self {
        arr.as_slice().into()
    }
}

impl<T: Integer> From<&[T]> for RangeSetInt<T> {
    fn from(slice: &[T]) -> Self {
        slice.iter().cloned().collect()
    }
}

impl<T, I> From<I> for RangeSetInt<T>
where
    T: Integer,
    // !!!cmk what does IntoIterator's ' IntoIter = I::IntoIter' mean?
    I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
    // cmk0 understand why this can't be  I: IntoIterator<Item = RangeInclusive<T>>, <I as IntoIterator>::IntoIter: SortedDisjoint, some conflict with from[]
{
    fn from(iter: I) -> Self {
        let mut iter_with_len = SortedDisjointWithLenSoFar::from(iter);
        let btree_map = BTreeMap::from_iter(&mut iter_with_len);
        RangeSetInt {
            btree_map,
            len: iter_with_len.len_so_far(),
        }
    }
}

impl<T, L, R> SortedStarts for Merge<T, L, R>
where
    T: Integer,
    L: Iterator<Item = RangeInclusive<T>> + SortedStarts,
    R: Iterator<Item = RangeInclusive<T>> + SortedStarts,
{
}

impl<T, I> SortedStarts for KMerge<T, I>
where
    T: Integer,
    I: Iterator<Item = RangeInclusive<T>> + SortedStarts,
{
}

pub type Merge<T, L, R> = MergeBy<L, R, fn(&RangeInclusive<T>, &RangeInclusive<T>) -> bool>;
pub type KMerge<T, I> = KMergeBy<I, fn(&RangeInclusive<T>, &RangeInclusive<T>) -> bool>;
pub type BitOrMerge<T, L, R> = SortedDisjointIter<T, Merge<T, L, R>>;
pub type BitOrKMerge<T, I> = SortedDisjointIter<T, KMerge<T, I>>;
pub type BitAndMerge<T, L, R> = NotIter<T, BitNandMerge<T, L, R>>;
pub type BitAndKMerge<T, I> = NotIter<T, BitNandKMerge<T, I>>;
pub type BitNandMerge<T, L, R> = BitOrMerge<T, NotIter<T, L>, NotIter<T, R>>;
pub type BitNandKMerge<T, I> = BitOrKMerge<T, NotIter<T, I>>;
pub type BitNorMerge<T, L, R> = NotIter<T, BitOrMerge<T, L, R>>;
pub type BitSubMerge<T, L, R> = NotIter<T, BitOrMerge<T, NotIter<T, L>, R>>;
pub type BitXOrTee<T, L, R> =
    BitOrMerge<T, BitSubMerge<T, Tee<L>, Tee<R>>, BitSubMerge<T, Tee<R>, Tee<L>>>;
pub type BitXOr<T, L, R> = BitOrMerge<T, BitSubMerge<T, L, Tee<R>>, BitSubMerge<T, Tee<R>, L>>;
pub type BitEq<T, L, R> = BitOrMerge<
    T,
    NotIter<T, BitOrMerge<T, NotIter<T, Tee<L>>, NotIter<T, Tee<R>>>>,
    NotIter<T, BitOrMerge<T, Tee<L>, Tee<R>>>,
>;

// !!!mk000 remove support for TryFrom from strings

pub fn union<T, I, J>(into_iter: I) -> BitOrKMerge<T, J::IntoIter>
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = RangeInclusive<T>>,
    J::IntoIter: SortedDisjoint,
    T: Integer,
{
    SortedDisjointIter::new(
        into_iter
            .into_iter()
            .kmerge_by(|pair0, pair1| pair0.start() <= pair1.start()),
    )
}

// cmk rule: don't for get these '+ SortedDisjoint'. They are easy to forget and hard to test, but must be tested (via "UI")
pub fn intersection<T, I, J>(into_iter: I) -> BitAndKMerge<T, J::IntoIter>
where
    // cmk rule prefer IntoIterator over Iterator (here is example)
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = RangeInclusive<T>>,
    J::IntoIter: SortedDisjoint,
    T: Integer,
{
    union(into_iter.into_iter().map(|seq| seq.into_iter().not())).not()
}

// define mathematical set methods, e.g. left_iter.left(right_iter) returns the left_iter.
pub trait SortedDisjointIterator<T: Integer>:
    Iterator<Item = RangeInclusive<T>> + SortedDisjoint + Sized
// I think this is 'Sized' because will sometimes want to create a struct (e.g. BitOrIter) that contains a field of this type
{
    fn bitor<R>(self, other: R) -> BitOrMerge<T, Self, R::IntoIter>
    where
        R: IntoIterator<Item = Self::Item>,
        R::IntoIter: SortedDisjoint,
    {
        SortedDisjointIter::new(self.merge_by(other.into_iter(), |a, b| a.start() <= b.start()))
    }

    fn bitand<R>(self, other: R) -> BitAndMerge<T, Self, R::IntoIter>
    where
        R: IntoIterator<Item = Self::Item>,
        R::IntoIter: SortedDisjoint,
    {
        !(self.not().bitor(other.into_iter().not()))
    }

    fn sub<R>(self, other: R) -> BitSubMerge<T, Self, R::IntoIter>
    where
        R: IntoIterator<Item = Self::Item>,
        R::IntoIter: SortedDisjoint,
    {
        !(self.not().bitor(other.into_iter()))
    }

    fn not(self) -> NotIter<T, Self> {
        NotIter::new(self)
    }

    // !!! cmk test the speed of this
    fn bitxor<R>(self, other: R) -> BitXOrTee<T, Self, R::IntoIter>
    where
        R: IntoIterator<Item = Self::Item>,
        R::IntoIter: SortedDisjoint,
    {
        let (lhs0, lhs1) = self.tee();
        let (rhs0, rhs1) = other.into_iter().tee();
        lhs0.sub(rhs0) | rhs1.sub(lhs1)
    }

    // cmk rule: Prefer IntoIterator to Iterator
    fn equal<R>(self, other: R) -> bool
    where
        R: IntoIterator<Item = Self::Item>,
        R::IntoIter: SortedDisjoint,
    {
        itertools::equal(self, other)
    }

    // cmk rule: You can't define traits on combinations of traits, so use this method to define methods on traits
    fn to_string(self) -> String {
        self.map(|range_inclusive| {
            let (start, stop) = range_inclusive.into_inner();
            format!("{start}..={stop}") // cmk could we format RangeInclusive directly?
        })
        .join(", ")
    }
}

// cmk0 explain why this is needed
impl<T, I> SortedDisjointIterator<T> for I
where
    T: Integer,
    I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
}

gen_ops_ex!(
    <T>;
    types ref RangeSetInt<T>, ref RangeSetInt<T> => RangeSetInt<T>;
    // Returns the union of `self` and `rhs` as a new `RangeSetInt`.
    //
    // # Examples
    //
    // ```
    // use range_set_int::RangeSetInt;
    //
    // let a = RangeSetInt::from([1, 2, 3]);
    // let b = RangeSetInt::from([3, 4, 5]);
    //
    // let result = &a | &b;
    // assert_eq!(result, RangeSetInt::from([1, 2, 3, 4, 5]));
    // let result = a | b;
    // assert_eq!(result, RangeSetInt::from([1, 2, 3, 4, 5]));
    // ```
    for | call |a: &RangeSetInt<T>, b: &RangeSetInt<T>| {
        (a.ranges()|b.ranges()).into()
    };
    for & call |a: &RangeSetInt<T>, b: &RangeSetInt<T>| {
        (a.ranges() & b.ranges()).into()
    };
    for ^ call |a: &RangeSetInt<T>, b: &RangeSetInt<T>| {
        (a.ranges() ^ b.ranges()).into()
    };
    for - call |a: &RangeSetInt<T>, b: &RangeSetInt<T>| {
        (a.ranges() - b.ranges()).into()
    };
    // cmk0 must/should we support both operators and methods?

    where T: Integer //Where clause for all impl's
);

gen_ops_ex!(
    <T>;
    types ref RangeSetInt<T> => RangeSetInt<T>;
    for ! call |a: &RangeSetInt<T>| {
        (!a.ranges()).into()
    };

    where T: Integer //Where clause for all impl's
);

impl<T: Integer> IntoIterator for RangeSetInt<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    /// Gets an iterator for moving out the `RangeSetInt`'s contents.
    ///
    /// # Examples
    ///
    /// ```
    /// use range_set_int::RangeSetInt;
    ///
    /// let set = RangeSetInt::from([1, 2, 3, 4]);
    ///
    /// let v: Vec<_> = set.into_iter().collect();
    /// assert_eq!(v, [1, 2, 3, 4]);
    /// ```
    fn into_iter(self) -> IntoIter<T> {
        IntoIter {
            option_range_inclusive: None,
            into_iter: self.btree_map.into_iter(),
        }
    }
}

#[derive(Clone)]
pub struct Iter<T, I>
where
    T: Integer,
    I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    iter: I,
    current: T, // !!!cmk can't we write this without current? (likewise IntoIter)
    option_range_inclusive: Option<RangeInclusive<T>>,
}

impl<T: Integer, I> Iterator for Iter<T, I>
where
    I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Item = T;
    fn next(&mut self) -> Option<T> {
        loop {
            if let Some(range_inclusive) = self.option_range_inclusive.clone() {
                let (start, stop) = range_inclusive.into_inner();
                debug_assert!(start <= stop && stop <= T::max_value2());
                self.current = start;
                if start < stop {
                    self.option_range_inclusive = Some(start + T::one()..=stop);
                } else {
                    self.option_range_inclusive = None;
                }
                return Some(self.current);
            } else if let Some(range_inclusive) = self.iter.next() {
                self.option_range_inclusive = Some(range_inclusive);
                continue;
            } else {
                return None;
            }
        }
    }

    // We'll have at least as many integers as intervals. There could be more that usize MAX
    // The option_range field could increase the number of integers, but we can ignore that.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (low, _high) = self.iter.size_hint();
        (low, None)
    }
}

pub struct IntoIter<T: Integer> {
    option_range_inclusive: Option<RangeInclusive<T>>,
    into_iter: std::collections::btree_map::IntoIter<T, T>,
}

impl<T: Integer> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(range_inclusive) = self.option_range_inclusive.clone() {
            let (start, stop) = range_inclusive.into_inner();
            debug_assert!(start <= stop && stop <= T::max_value2());
            if start < stop {
                self.option_range_inclusive = Some(start + T::one()..=stop);
            } else {
                self.option_range_inclusive = None;
            }
            Some(start)
        } else if let Some((start, stop)) = self.into_iter.next() {
            self.option_range_inclusive = Some(start..=stop);
            self.next() // will recurse at most once
        } else {
            None
        }
    }

    // We'll have at least as many integers as intervals. There could be more that usize MAX
    // the option_range field could increase the number of integers, but we can ignore that.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (low, _high) = self.into_iter.size_hint();
        (low, None)
    }
}

/// cmk warn that adds one-by-one
impl<T: Integer> Extend<T> for RangeSetInt<T> {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        let iter = iter.into_iter();
        for range_inclusive in UnsortedDisjoint::from(iter.map(|x| x..=x)) {
            self.internal_add(range_inclusive);
        }
    }
}

impl<'a, T: 'a + Integer> Extend<&'a T> for RangeSetInt<T> {
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        self.extend(iter.into_iter().cloned());
    }
}

// !!!cmk support =, and single numbers
// !!!cmk error to use -
// !!!cmk are the unwraps OK?
// !!!cmk what about bad input?

// !!!cmk0 just "Error" or remove if unused?
#[derive(ThisError, Debug)]
pub enum RangeIntSetError {
    // #[error("after splitting on ',' tried to split on '..=' but failed on {0}")]
    // ParseSplitError(String),
    // #[error("error parsing integer {0}")]
    // ParseIntegerError(String),
}

// impl<T: Integer> FromStr for RangeSetInt<T>
// where
//     // !!! cmk understand this
//     <T as std::str::FromStr>::Err: std::fmt::Debug,
// {
//     type Err = RangeIntSetError;

//     fn from_str(s: &str) -> Result<Self, Self::Err> {
//         if s.is_empty() {
//             return Ok(RangeSetInt::new());
//         }
//         let result: Result<RangeSetInt<T>, Self::Err> = s.split(',').map(process_bit1).collect();
//         result
//     }
// }

// // !!!cmk00 test all errors
// // !!!cmk00 rename
// fn process_bit1<T: Integer>(s: &str) -> Result<RangeInclusive<T>, RangeIntSetError>
// where
//     <T as std::str::FromStr>::Err: std::fmt::Debug,
// {
//     let mut range = s.split("..=");
//     let start = range
//         .next()
//         .ok_or(RangeIntSetError::ParseSplitError("first item".to_string()))?;
//     let start_result = start.parse::<T>();
//     match start_result {
//         Ok(start) => {
//             let stop = range
//                 .next()
//                 .ok_or(RangeIntSetError::ParseSplitError("second item".to_string()))?;
//             let stop_result = stop.parse::<T>();
//             match stop_result {
//                 Ok(stop) => {
//                     if range.next().is_some() {
//                         Err(RangeIntSetError::ParseSplitError(
//                             "unexpected third item".to_string(),
//                         ))
//                     } else {
//                         Ok(start..=stop)
//                     }
//                 }
//                 Err(e) => {
//                     let msg = format!("second item: {e:?}");
//                     Err(RangeIntSetError::ParseIntegerError(msg))
//                 }
//             }
//         }
//         Err(e) => {
//             let msg = format!("first item: {e:?}");
//             Err(RangeIntSetError::ParseIntegerError(msg))
//         }
//     }
// }

// fn process_bit2<T: Integer>(
//     mut range: std::str::Split<&str>,
//     start: T,
// ) -> Result<RangeInclusive<T>, RangeIntSetError>
// where
//     <T as std::str::FromStr>::Err: std::fmt::Debug,
// {
//     let stop = range.next().ok_or(RangeIntSetError::ParseSplitError)?;
//     let stop_result = stop.parse::<T>();
//     match stop_result {
//         Ok(stop) => Ok((start, stop)),
//         Err(e) => {
//             let msg = format!("{e:?}");
//             Err(RangeIntSetError::ParseIntegerError(msg))
//         }
//     }
// }

// impl<T: Integer> TryFrom<&str> for RangeSetInt<T>
// where
//     // !!! cmk understand this
//     <T as std::str::FromStr>::Err: std::fmt::Debug,
// {
//     type Error = RangeIntSetError;
//     fn try_from(s: &str) -> Result<Self, Self::Error> {
//         FromStr::from_str(s)
//     }
// }

pub trait SortedStarts {}
pub trait SortedDisjoint: SortedStarts {}

// cmk This code from sorted-iter shows how to define clone when possible
// impl<I: Iterator + Clone, J: Iterator + Clone> Clone for Union<I, J>
// where
//     I::Item: Clone,
//     J::Item: Clone,
// {
//     fn clone(&self) -> Self {
//         Self {
//             a: self.a.clone(),
//             b: self.b.clone(),
//         }
//     }
// }

// cmk sort-iter uses peekable. Is that better?

pub struct DynSortedDisjoint<'a, T> {
    iter: Box<dyn Iterator<Item = T> + 'a>,
}

// All DynSortedDisjoint's are SortedDisjoint's
impl<'a, T> SortedStarts for DynSortedDisjoint<'a, T> {}
impl<'a, T> SortedDisjoint for DynSortedDisjoint<'a, T> {}

impl<'a, T> Iterator for DynSortedDisjoint<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    // cmk rule Implement size_hint if possible and ExactSizeIterator if possible
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// extension trait for any iterator to add a assume_sorted_by_item method
pub trait DynSortedDisjointExt<'a>: Iterator + SortedDisjoint + Sized + 'a {
    /// create dynamic version of the iterator
    fn dyn_sorted_disjoint(self) -> DynSortedDisjoint<'a, Self::Item> {
        DynSortedDisjoint {
            iter: Box::new(self),
        }
    }
}

// !!!cmk understand this
impl<'a, I: Iterator + SortedDisjoint + 'a> DynSortedDisjointExt<'a> for I {}

#[macro_export]
macro_rules! intersection_dyn {
    ($($val:expr),*) => {intersection([$($val.dyn_sorted_disjoint()),*])}
}

#[macro_export]
macro_rules! union_dyn {
    ($($val:expr),*) => {union([$($val.dyn_sorted_disjoint()),*])}
}

// Not: Ranges, NotIter, BitOrMerge
impl<T: Integer> ops::Not for Ranges<'_, T> {
    type Output = NotIter<T, Self>;

    fn not(self) -> Self::Output {
        NotIter::new(self)
    }
}

impl<T: Integer, I> ops::Not for NotIter<T, I>
where
    I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = NotIter<T, Self>;

    fn not(self) -> Self::Output {
        // It would be fun to optimize to self.iter, but that would require
        // also considering fields 'start_not' and 'next_time_return_none'.
        NotIter::new(self)
    }
}

impl<T: Integer, I> ops::Not for SortedDisjointIter<T, I>
where
    I: Iterator<Item = RangeInclusive<T>> + SortedStarts,
{
    type Output = NotIter<T, Self>;

    fn not(self) -> Self::Output {
        NotIter::new(self)
    }
}

// BitOr: Ranges, NotIter, BitOrMerge
impl<T: Integer, I> ops::BitOr<I> for Ranges<'_, T>
where
    I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = BitOrMerge<T, Self, I>;

    fn bitor(self, rhs: I) -> Self::Output {
        SortedDisjointIterator::bitor(self, rhs)
    }
}

impl<T: Integer, R, L> ops::BitOr<R> for NotIter<T, L>
where
    L: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
    R: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = BitOrMerge<T, Self, R>;

    fn bitor(self, rhs: R) -> Self::Output {
        SortedDisjointIterator::bitor(self, rhs)
    }
}

impl<T: Integer, R, L> ops::BitOr<R> for SortedDisjointIter<T, L>
where
    L: Iterator<Item = RangeInclusive<T>> + SortedStarts,
    R: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = BitOrMerge<T, Self, R>;

    fn bitor(self, rhs: R) -> Self::Output {
        // It might be fine to optimize to self.iter, but that would require
        // also considering field 'range'
        SortedDisjointIterator::bitor(self, rhs)
    }
}

// Sub: Ranges, NotIter, BitOrMerge

impl<T: Integer, I> ops::Sub<I> for Ranges<'_, T>
where
    I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = BitSubMerge<T, Self, I>;

    fn sub(self, rhs: I) -> Self::Output {
        !(!self | rhs)
    }
}

impl<T: Integer, R, L> ops::Sub<R> for NotIter<T, L>
where
    L: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
    R: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = BitSubMerge<T, Self, R>;

    fn sub(self, rhs: R) -> Self::Output {
        // It would be fun to optimize !!self.iter into self.iter
        // but that would require also considering fields 'start_not' and 'next_time_return_none'.
        !(!self | rhs)
    }
}

impl<T: Integer, R, L> ops::Sub<R> for SortedDisjointIter<T, L>
where
    L: Iterator<Item = RangeInclusive<T>> + SortedStarts,
    R: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = BitSubMerge<T, Self, R>;

    fn sub(self, rhs: R) -> Self::Output {
        !(!self | rhs)
    }
}

// BitXor: Ranges, NotIter, BitOrMerge

impl<T: Integer, I> ops::BitXor<I> for Ranges<'_, T>
where
    I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = BitXOr<T, Self, I>;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn bitxor(self, rhs: I) -> Self::Output {
        // We optimize by using self.clone() instead of tee
        let lhs1 = self.clone();
        let (rhs0, rhs1) = rhs.tee();
        (self - rhs0) | (rhs1.sub(lhs1))
    }
}

impl<T: Integer, R, L> ops::BitXor<R> for NotIter<T, L>
where
    L: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
    R: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = BitXOrTee<T, Self, R>;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn bitxor(self, rhs: R) -> Self::Output {
        // It would be fine optimize !!self.iter into self.iter, ala
        // ¬(¬n ∨ ¬r) ∨ ¬(n ∨ r) // https://www.wolframalpha.com/input?i=%28not+n%29+xor+r
        // but that would require also considering fields 'start_not' and 'next_time_return_none'.
        let (lhs0, lhs1) = self.tee();
        let (rhs0, rhs1) = rhs.tee();
        lhs0.sub(rhs0) | rhs1.sub(lhs1)
    }
}

impl<T: Integer, R, L> ops::BitXor<R> for SortedDisjointIter<T, L>
where
    L: Iterator<Item = RangeInclusive<T>> + SortedStarts,
    R: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = BitXOrTee<T, Self, R>;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn bitxor(self, rhs: R) -> Self::Output {
        let (lhs0, lhs1) = self.tee();
        let (rhs0, rhs1) = rhs.tee();
        lhs0.sub(rhs0) | rhs1.sub(lhs1)
    }
}

// BitAnd: Ranges, NotIter, BitOrMerge

impl<T: Integer, I> ops::BitAnd<I> for Ranges<'_, T>
where
    I: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = BitAndMerge<T, Self, I>;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn bitand(self, rhs: I) -> Self::Output {
        !(!self | rhs.not())
    }
}

impl<T: Integer, R, L> ops::BitAnd<R> for NotIter<T, L>
where
    L: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
    R: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = BitAndMerge<T, Self, R>;

    fn bitand(self, rhs: R) -> Self::Output {
        // It would be fun to optimize !!self.iter into self.iter
        // but that would require also considering fields 'start_not' and 'next_time_return_none'.
        !(!self | rhs.not())
    }
}

// cmk name all generics in a sensible way
impl<T: Integer, R, L> ops::BitAnd<R> for SortedDisjointIter<T, L>
where
    L: Iterator<Item = RangeInclusive<T>> + SortedStarts,
    R: Iterator<Item = RangeInclusive<T>> + SortedDisjoint,
{
    type Output = BitAndMerge<T, Self, R>;

    fn bitand(self, rhs: R) -> Self::Output {
        !(!self | rhs.not())
    }
}
